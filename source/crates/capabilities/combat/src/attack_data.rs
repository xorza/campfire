use std::collections::BTreeMap;
use std::sync::Arc;

use campfire_actions::{ActionKind, KindSection, KindSections, KindTakes, RangeTakes, Takes};
use campfire_common::DeclaredName;
use campfire_declarations::Listed;
use campfire_geometry::MAX_BOUND;
use campfire_math::Num;
use campfire_script::ApiBuilder;
use campfire_sim::Capability;
use campfire_values::{Number, Ranked, RelationSet, Speed, Stat};
use serde::de::Error;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The section of an action of kind `attack`, a weapon: where its rate and its damage come from,
/// the kind of damage it deals, its clip, its projectile's splash and scatter, its bound of pitch,
/// and the speed that delays the hit of a weapon of no projectile
/// (04-capabilities/actions.md, Each kind owns its section).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttackData {
    /// Its attacks a second, a stat `rate`, or its own time from one attack's start to the next,
    /// `period_ms`.
    pub rate: WeaponValue,
    /// Its damage, a stat `damage`, or its own `amount`.
    pub damage: WeaponValue,
    pub damage_kind: DeclaredName,
    pub clip: Option<ClipData>,
    pub splash: Option<SplashData>,
    pub scatter: Option<ScatterData>,
    pub pitch_deg: Option<PitchData>,
    /// How fast a weapon of no projectile reaches its target, which delays its hit
    /// (04-capabilities/combat.md, A slow weapon delays its damage).
    pub speed: Option<Speed>,
}

/// Where a value of a weapon comes from, as its data gives it: a stat of its attacker, or its own
/// number at each rank.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WeaponValue {
    Stat(Stat),
    Own(Ranked<Number>),
}

impl WeaponValue {
    /// Its own number at each rank, when it is not a stat's.
    pub const fn own(&self) -> Option<&Ranked<Number>> {
        match self {
            Self::Own(own) => Some(own),
            Self::Stat(_) => None,
        }
    }
}

/// Data is untrusted, so a weapon of both or neither of a stat and its own value, for its rate or
/// its damage, fails to read.
impl<'de> Deserialize<'de> for AttackData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            rate: Option<Stat>,
            damage: Option<Stat>,
            damage_kind: DeclaredName,
            amount: Option<Ranked<Number>>,
            period_ms: Option<Ranked<Number>>,
            clip: Option<ClipData>,
            splash: Option<SplashData>,
            scatter: Option<ScatterData>,
            pitch_deg: Option<PitchData>,
            speed: Option<Speed>,
        }
        let fields = Fields::deserialize(deserializer)?;
        let value = |stat, own, message| match (stat, own) {
            (Some(stat), None) => Ok(WeaponValue::Stat(stat)),
            (None, Some(own)) => Ok(WeaponValue::Own(own)),
            (Some(_), Some(_)) | (None, None) => Err(D::Error::custom(message)),
        };
        Ok(Self {
            rate: value(
                fields.rate,
                fields.period_ms,
                "a weapon's rate is a stat, `rate`, or its own `period_ms`: one of them",
            )?,
            damage: value(
                fields.damage,
                fields.amount,
                "a weapon's damage is a stat, `damage`, or its own `amount`: one of them",
            )?,
            damage_kind: fields.damage_kind,
            clip: fields.clip,
            splash: fields.splash,
            scatter: fields.scatter,
            pitch_deg: fields.pitch_deg,
            speed: fields.speed,
        })
    }
}

/// As data writes it, what `Deserialize` reads: its damage, `amount` or the stat `damage`, its
/// kind, its rate, `period_ms` or the stat `rate`, and each part it has.
impl Serialize for AttackData {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        match &self.damage {
            WeaponValue::Own(amount) => map.serialize_entry("amount", amount)?,
            WeaponValue::Stat(stat) => map.serialize_entry("damage", stat)?,
        }
        map.serialize_entry("damage_kind", &self.damage_kind)?;
        match &self.rate {
            WeaponValue::Own(period) => map.serialize_entry("period_ms", period)?,
            WeaponValue::Stat(stat) => map.serialize_entry("rate", stat)?,
        }
        if let Some(clip) = &self.clip {
            map.serialize_entry("clip", clip)?;
        }
        if let Some(splash) = &self.splash {
            map.serialize_entry("splash", splash)?;
        }
        if let Some(scatter) = &self.scatter {
            map.serialize_entry("scatter", scatter)?;
        }
        if let Some(pitch) = &self.pitch_deg {
            map.serialize_entry("pitch_deg", &[pitch.low, pitch.high])?;
        }
        if let Some(speed) = &self.speed {
            map.serialize_entry("speed", speed)?;
        }
        map.end()
    }
}

impl KindSection for AttackData {
    fn kind(&self) -> ActionKind {
        ActionKind::Attack
    }

    fn needs(&self) -> &'static [Capability] {
        &[Capability::Combat]
    }

    /// A weapon's period stands for a cooldown, and its hit for a resolve: it takes no script,
    /// no time of a cast, and no list but `on_hit`.
    fn takes(&self) -> KindTakes {
        KindTakes {
            range: RangeTakes::Meters,
            cost: Takes::May,
            windup: Takes::May,
            passive: Takes::May,
            delivery: Takes::May,
            params: Takes::May,
            on_hit: Takes::May,
            ..KindTakes::NONE
        }
    }

    fn numbers(&self) -> Vec<&Ranked<Number>> {
        let own = [&self.rate, &self.damage].into_iter();
        let own = own.filter_map(|value| value.own());
        let clip = self.clip.iter();
        let clip = clip.flat_map(|clip| [&clip.shots, &clip.reload_ms]);
        own.chain(clip).collect()
    }
}

impl Listed for AttackData {
    type Set = KindSections;

    const KEY: &'static str = ActionKind::Attack.name();

    fn into_listed(self) -> Arc<dyn KindSection> {
        Arc::new(self)
    }

    fn api(_: &mut ApiBuilder<'_>) {}
}

/// A weapon's `clip`: the shots it fires before it reloads, and the time it reloads for, from
/// the shot that empties it, in place of its period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipData {
    pub shots: Ranked<Number>,
    pub reload_ms: Ranked<Number>,
}

/// A weapon's `splash` (04-capabilities/combat.md, An attack may splash): where its projectile
/// ends, its damage to each living unit of the relations `affects` names whose volume lies
/// within `radius` meters, and to its source unless `spares_source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SplashData {
    pub(crate) radius: Num,
    pub(crate) affects: RelationSet,
    pub(crate) spares_source: bool,
}

/// A weapon's `scatter` (04-capabilities/combat.md, An attack may scatter): the most meters
/// from its target's ground point its projectile aims at, `radius` and, for a target of a tag
/// `tags` names, the largest of their radii more.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScatterData {
    pub(crate) radius: Num,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) tags: BTreeMap<DeclaredName, Num>,
}

/// A weapon's `pitch_deg` (04-capabilities/combat.md, An attack may bound its pitch): the
/// least and the most angle, in degrees above the ground plane, at which its target's center
/// lies from its attacker's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PitchData {
    pub(crate) low: Num,
    pub(crate) high: Num,
}

impl SplashData {
    /// A splash of `radius` to the units of `affects`, and to its source unless `spares_source`;
    /// `None` for a radius that is not positive or past `MAX_BOUND`.
    pub fn new(radius: Num, affects: RelationSet, spares_source: bool) -> Option<Self> {
        (radius > Num::ZERO && radius <= MAX_BOUND).then_some(Self {
            radius,
            affects,
            spares_source,
        })
    }
}

impl ScatterData {
    /// A scatter of `radius`, and of each of `tags` more; `None` for a radius below 0 or past
    /// `MAX_BOUND`.
    pub fn new(radius: Num, tags: BTreeMap<DeclaredName, Num>) -> Option<Self> {
        let holds = |radius: &Num| (Num::ZERO..=MAX_BOUND).contains(radius);
        (holds(&radius) && tags.values().all(holds)).then_some(Self { radius, tags })
    }
}

impl PitchData {
    /// The bound from `low` to `high` degrees; `None` below −90° or above 90°, for a least past
    /// the most, or for a least of 90° or a most of −90°, which would hold one pitch, which the
    /// test of a bound, by the sine of the pitch past it, cannot tell from the opposite one.
    pub fn new(low: Num, high: Num) -> Option<Self> {
        let quarter = Num::int(90);
        let refused =
            low < -quarter || high > quarter || low > high || low == quarter || high == -quarter;
        (!refused).then_some(Self { low, high })
    }
}

/// Data is untrusted, so a radius that is not positive or past `MAX_BOUND` fails to read.
impl<'de> Deserialize<'de> for SplashData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            radius: Num,
            affects: RelationSet,
            spares_source: Option<bool>,
        }
        let fields = Fields::deserialize(deserializer)?;
        let spares_source = fields.spares_source.unwrap_or(true);
        Self::new(fields.radius, fields.affects, spares_source)
            .ok_or_else(|| D::Error::custom("a splash's radius is positive, up to 2,048 m"))
    }
}

/// Data is untrusted, so a radius below 0 or past `MAX_BOUND` fails to read.
impl<'de> Deserialize<'de> for ScatterData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(default)]
            radius: Num,
            #[serde(default)]
            tags: BTreeMap<DeclaredName, Num>,
        }
        let fields = Fields::deserialize(deserializer)?;
        Self::new(fields.radius, fields.tags)
            .ok_or_else(|| D::Error::custom("a scatter's radius is from 0 to 2,048 m"))
    }
}

/// Data is untrusted, so a pitch below −90° or above 90°, or a least one past the most, fails to
/// read.
impl<'de> Deserialize<'de> for PitchData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let [low, high] = <[Num; 2]>::deserialize(deserializer)?;
        Self::new(low, high).ok_or_else(|| {
            D::Error::custom(
                "a pitch's bound is two angles from -90 to 90 degrees, the least first and \
                 below 90, the most above -90",
            )
        })
    }
}
