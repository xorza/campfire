use std::collections::BTreeMap;
use std::sync::Arc;

use campfire_actions::{
    ActionKind, KindSection, KindSections, KindTakes, ListedSpec, PipelineField, Takes,
};
use campfire_common::DeclaredName;
use campfire_declarations::Listed;
use campfire_geometry::MAX_BOUND;
use campfire_math::Num;
use campfire_script::ApiBuilder;
use campfire_script::{ApiVersion, Status};
use campfire_sim::Capability;
use campfire_values::{Number, Ranked, RelationSet, Speed, Stat, TagFilterData};
use serde::de::Error;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::bonus_stats::BonusData;

/// The section of an action of kind `attack`, a weapon: where its rate and its damage come from,
/// the stat that speeds its own period, the kind of damage it deals, the filter of the targets it
/// prefers, its clip, its projectile's splash and scatter, its bound of pitch, the speed that
/// delays the hit of a weapon of no projectile, its shot streak, its pre-attack, and the bearing
/// its attacker aims within (04-capabilities/actions.md, Each kind owns its section).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttackData {
    /// Its attacks a second, a stat `rate`, or its own time from one attack's start to the next,
    /// `period_ms`.
    pub rate: WeaponValue,
    /// The stats whose sum speeds its own period and its clip's reload, divided by 1 plus the
    /// sum (04-capabilities/combat.md, A weapon's own period may speed up by a stat).
    pub rate_bonus: Option<BonusData>,
    /// Its damage, a stat `damage`, or its own `amount`.
    pub damage: WeaponValue,
    /// The stats whose sum scales its own amount and its splash's outer amount, times 1 plus the
    /// sum (04-capabilities/combat.md, One bonus scales both rings).
    pub damage_bonus: Option<BonusData>,
    /// The stats whose sum stretches its range, times 1 plus the sum (04-capabilities/combat.md, A
    /// container's modifier holds on its riders).
    pub range_bonus: Option<BonusData>,
    pub damage_kind: DeclaredName,
    /// The tags of the targets it is chosen for above every weapon that does not prefer them
    /// (04-capabilities/combat.md, An attack may prefer its targets).
    pub prefers: Option<TagFilterData>,
    pub clip: Option<ClipData>,
    pub splash: Option<SplashData>,
    pub scatter: Option<ScatterData>,
    pub pitch_deg: Option<PitchData>,
    /// How fast a weapon of no projectile reaches its target, which delays its hit
    /// (04-capabilities/combat.md, A slow weapon delays its damage).
    pub speed: Option<Speed>,
    pub streak: Option<StreakData>,
    pub pre_attack: Option<PreAttackData>,
    /// The degrees from its target's bearing within which its attacker, turning toward it, starts
    /// it, when no turret carries it (04-capabilities/combat.md, An attacker aims its body).
    pub aim_deg: Option<Num>,
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
/// its damage, or of an aim outside 0 to 180 degrees, fails to read.
impl<'de> Deserialize<'de> for AttackData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            rate: Option<Stat>,
            rate_bonus: Option<BonusData>,
            damage: Option<Stat>,
            damage_bonus: Option<BonusData>,
            range_bonus: Option<BonusData>,
            damage_kind: DeclaredName,
            prefers: Option<TagFilterData>,
            amount: Option<Ranked<Number>>,
            period_ms: Option<Ranked<Number>>,
            clip: Option<ClipData>,
            splash: Option<SplashData>,
            scatter: Option<ScatterData>,
            pitch_deg: Option<PitchData>,
            speed: Option<Speed>,
            streak: Option<StreakData>,
            pre_attack: Option<PreAttackData>,
            aim_deg: Option<Num>,
        }
        let fields = Fields::deserialize(deserializer)?;
        if fields
            .aim_deg
            .is_some_and(|aim| !(Num::ZERO..=Num::int(180)).contains(&aim))
        {
            return Err(D::Error::custom(
                "a weapon's aim is from 0 to 180 degrees of its target's bearing",
            ));
        }
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
            rate_bonus: fields.rate_bonus,
            damage: value(
                fields.damage,
                fields.amount,
                "a weapon's damage is a stat, `damage`, or its own `amount`: one of them",
            )?,
            damage_bonus: fields.damage_bonus,
            range_bonus: fields.range_bonus,
            damage_kind: fields.damage_kind,
            prefers: fields.prefers,
            clip: fields.clip,
            splash: fields.splash,
            scatter: fields.scatter,
            pitch_deg: fields.pitch_deg,
            speed: fields.speed,
            streak: fields.streak,
            pre_attack: fields.pre_attack,
            aim_deg: fields.aim_deg,
        })
    }
}

/// As data writes it, what `Deserialize` reads: its damage, `amount` or the stat `damage`, its
/// kind, the filter it prefers, its rate, `period_ms` or the stat `rate`, and each optional field
/// it has.
impl Serialize for AttackData {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        match &self.damage {
            WeaponValue::Own(amount) => map.serialize_entry("amount", amount)?,
            WeaponValue::Stat(stat) => map.serialize_entry("damage", stat)?,
        }
        if let Some(stat) = &self.damage_bonus {
            map.serialize_entry("damage_bonus", stat)?;
        }
        map.serialize_entry("damage_kind", &self.damage_kind)?;
        if let Some(prefers) = &self.prefers {
            map.serialize_entry("prefers", prefers)?;
        }
        match &self.rate {
            WeaponValue::Own(period) => map.serialize_entry("period_ms", period)?,
            WeaponValue::Stat(stat) => map.serialize_entry("rate", stat)?,
        }
        if let Some(stat) = &self.rate_bonus {
            map.serialize_entry("rate_bonus", stat)?;
        }
        if let Some(stat) = &self.range_bonus {
            map.serialize_entry("range_bonus", stat)?;
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
        if let Some(pre_attack) = &self.pre_attack {
            map.serialize_entry("pre_attack", pre_attack)?;
        }
        if let Some(streak) = &self.streak {
            map.serialize_entry("streak", streak)?;
        }
        if let Some(aim) = &self.aim_deg {
            map.serialize_entry("aim_deg", aim)?;
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
        KindTakes::NONE
            .with(Takes::Must, &[PipelineField::Range])
            .with(
                Takes::May,
                &[
                    PipelineField::Cost,
                    PipelineField::WindupMs,
                    PipelineField::PassiveModifier,
                    PipelineField::Delivery,
                    PipelineField::Params,
                    PipelineField::OnHit,
                ],
            )
    }

    fn numbers(&self) -> Vec<&Ranked<Number>> {
        let own = [&self.rate, &self.damage].into_iter();
        let own = own.filter_map(|value| value.own());
        let clip = self.clip.iter();
        let clip = clip.flat_map(|clip| [&clip.shots, &clip.reload_ms]);
        let outer = self
            .splash
            .iter()
            .filter_map(|splash| splash.outer.as_ref());
        let pre_attack = self.pre_attack.iter().map(|pre_attack| &pre_attack.ms);
        own.chain(clip)
            .chain(outer.map(|outer| &outer.amount))
            .chain(pre_attack)
            .collect()
    }

    fn resolves(&self) -> Option<Arc<dyn ListedSpec>> {
        None
    }
}

impl Listed for AttackData {
    type Set = KindSections;

    const KEY: &'static str = ActionKind::Attack.name();

    const STATUS: Status = Status::Runs(ApiVersion::FIRST);

    fn into_listed(self) -> Arc<dyn KindSection> {
        Arc::new(self)
    }

    fn api(_: &mut ApiBuilder<'_>) {}
}

/// A weapon's `clip`: the shots it fires before it reloads, the time it reloads for, and how it
/// refills: by that time from the shot that empties it, in place of its period, or at its unit's
/// home alone (04-capabilities/combat.md, Clips; 04-capabilities/combat.md, A clip may reload at
/// home).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipData {
    pub shots: Ranked<Number>,
    pub reload_ms: Ranked<Number>,
    #[serde(default, skip_serializing_if = "ClipRefill::is_time")]
    pub refill: ClipRefill,
}

/// How a clip refills once empty: by its reload's time, or only at its unit's home, where the
/// capability that keeps the home refills it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipRefill {
    #[default]
    Time,
    Home,
}

impl ClipRefill {
    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde's test of a field to skip takes it by reference"
    )]
    const fn is_time(&self) -> bool {
        matches!(self, Self::Time)
    }
}

/// A weapon's `pre_attack` (04-capabilities/combat.md, A pre-attack): the time its unit aims
/// before a start of the weapon, and what starts it again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreAttackData {
    pub ms: Ranked<Number>,
    pub each: PreAttackEach,
}

/// What starts a weapon's pre-attack again, as Zero Hour's `PreAttackType`: each shot
/// (`PER_SHOT`), each target (`PER_ATTACK`), or each clip (`PER_CLIP`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreAttackEach {
    Shot,
    Target,
    Clip,
}

/// A weapon's `streak` (04-capabilities/combat.md, A shot streak gives a stage's
/// modifier): how long after the weapon is ready again a unit's streak of its shots holds
/// without a shot, its stages, each the shots past which its unit holds the stage's modifier,
/// and its fall, if it has one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreakData {
    pub coast_ms: u64,
    pub stages: Vec<StageData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fall: Option<FallData>,
}

/// A streak's `fall` (04-capabilities/combat.md, A streak's fall holds a modifier): the modifier
/// of the weapon's package its unit holds from a fall of its stage, and how long after a fall
/// past the coast it holds without a shot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FallData {
    pub modifier: DeclaredName,
    pub ms: u64,
}

/// A stage of a weapon's streak: the shots in a row its unit fires past, and the modifier of the
/// weapon's package it then holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageData {
    pub shots: u32,
    pub modifier: DeclaredName,
}

/// A weapon's `splash` (04-capabilities/combat.md, An attack may splash): where its projectile
/// ends, its damage to each living unit of the relations `affects` names whose volume lies
/// within `radius` meters, and to its source unless `spares_source`; and, with `outer`, its
/// outer ring's amount to each such unit beyond `radius` and within the ring's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SplashData {
    pub(crate) radius: Num,
    pub(crate) affects: RelationSet,
    pub(crate) spares_source: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) outer: Option<OuterData>,
}

/// A splash's `outer` ring (04-capabilities/combat.md, A splash may have an outer ring): its radius
/// in meters, past its splash's, and the amount it deals at each rank, at least 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OuterData {
    pub(crate) radius: Num,
    pub(crate) amount: Ranked<Number>,
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
    /// `None` for a radius below 0 or past `MAX_BOUND`. A radius of 0 reaches the unit whose
    /// volume holds its point alone, as Zero Hour's rocket buggy's of no primary radius does.
    pub fn new(radius: Num, affects: RelationSet, spares_source: bool) -> Option<Self> {
        (Num::ZERO..=MAX_BOUND).contains(&radius).then_some(Self {
            radius,
            affects,
            spares_source,
            outer: None,
        })
    }

    /// The same splash with the outer ring `outer`; `None` for a ring not past its radius or past
    /// `MAX_BOUND`.
    #[must_use]
    pub fn with_outer(self, outer: OuterData) -> Option<Self> {
        (self.radius < outer.radius && outer.radius <= MAX_BOUND).then_some(Self {
            outer: Some(outer),
            ..self
        })
    }
}

impl OuterData {
    /// A ring of `radius` meters dealing `amount` at each rank.
    pub const fn new(radius: Num, amount: Ranked<Number>) -> Self {
        Self { radius, amount }
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

/// Data is untrusted, so a radius below 0 or past `MAX_BOUND`, or an outer ring's not past it or
/// past `MAX_BOUND`, fails to read.
impl<'de> Deserialize<'de> for SplashData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            radius: Num,
            affects: RelationSet,
            spares_source: Option<bool>,
            outer: Option<OuterData>,
        }
        let fields = Fields::deserialize(deserializer)?;
        let spares_source = fields.spares_source.unwrap_or(true);
        let splash = Self::new(fields.radius, fields.affects, spares_source)
            .ok_or_else(|| D::Error::custom("a splash's radius is from 0 to 2,048 m"))?;
        match fields.outer {
            None => Ok(splash),
            Some(outer) => splash.with_outer(outer).ok_or_else(|| {
                D::Error::custom("a splash's outer ring is past its radius, up to 2,048 m")
            }),
        }
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
