use std::collections::BTreeMap;
use std::sync::Arc;

use campfire_actions::{ActionKind, KindSection, KindSections, KindTakes, RangeTakes, Takes};
use campfire_common::DeclaredName;
use campfire_declare::Listed;
use campfire_geometry::MAX_BOUND;
use campfire_math::Num;
use campfire_sim::Capability;
use campfire_values::{Number, Ranked, RelationSet, Speed, Stat};
use serde::de::Error;
use serde::{Deserialize, Deserializer};

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
}

/// A weapon's `clip`: the shots it fires before it reloads, and the time it reloads for, from
/// the shot that empties it, in place of its period.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipData {
    pub shots: Ranked<Number>,
    pub reload_ms: Ranked<Number>,
}

/// A weapon's `splash` (04-capabilities/combat.md, An attack may splash): where its projectile
/// ends, its damage to each living unit of the relations `affects` names whose volume lies
/// within `radius` meters, and to its source unless `spares_source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplashData {
    pub radius: Num,
    pub affects: RelationSet,
    pub spares_source: bool,
}

/// A weapon's `scatter` (04-capabilities/combat.md, An attack may scatter): the most meters
/// from its target's ground point its projectile aims at, `radius` and, for a target of a tag
/// `tags` names, the largest of their radii more.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScatterData {
    pub radius: Num,
    pub tags: BTreeMap<DeclaredName, Num>,
}

/// A weapon's `pitch_deg` (04-capabilities/combat.md, An attack may bound its pitch): the
/// least and the most angle, in degrees above the ground plane, at which its target's center
/// lies from its attacker's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PitchData {
    pub low: Num,
    pub high: Num,
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
        if fields.radius <= Num::ZERO || fields.radius > MAX_BOUND {
            return Err(D::Error::custom(
                "a splash's radius is positive, up to 2,048 m",
            ));
        }
        Ok(Self {
            radius: fields.radius,
            affects: fields.affects,
            spares_source: fields.spares_source.unwrap_or(true),
        })
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
        let holds = |radius: &Num| (Num::ZERO..=MAX_BOUND).contains(radius);
        if !holds(&fields.radius) || !fields.tags.values().all(holds) {
            return Err(D::Error::custom("a scatter's radius is from 0 to 2,048 m"));
        }
        Ok(Self {
            radius: fields.radius,
            tags: fields.tags,
        })
    }
}

/// Data is untrusted, so a pitch below −90° or above 90°, or a least one past the most, fails to
/// read.
impl<'de> Deserialize<'de> for PitchData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let [low, high] = <[Num; 2]>::deserialize(deserializer)?;
        // A least of 90° or a most of −90° would hold one pitch, which the test of a bound,
        // by the sine of the pitch past it, cannot tell from the opposite one.
        let quarter = Num::int(90);
        if low < -quarter || high > quarter || low > high || low == quarter || high == -quarter {
            return Err(D::Error::custom(
                "a pitch's bound is two angles from -90 to 90 degrees, the least first and \
                 below 90, the most above -90",
            ));
        }
        Ok(Self { low, high })
    }
}
