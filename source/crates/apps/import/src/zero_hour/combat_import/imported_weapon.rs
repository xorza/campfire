use std::collections::BTreeMap;

use campfire_actions::{ActionKeys, ActionRange, RangeField, Targeting};
use campfire_codec::TomlTable;
use campfire_combat::{
    AttackData, BonusData, ClipData, ClipRefill, FallData, OuterData, PitchData, PreAttackData,
    PreAttackEach, ScatterData, SplashData, StageData, StreakData, WeaponValue,
};
use campfire_common::DeclaredName;
use campfire_math::{Num, Rounding};
use campfire_projectiles::ProjectileDeliveryData;
use campfire_values::{Number, Ranked, RelationSet, Scalar, Speed, Stat};

use crate::zero_hour::combat_import::ConvertedWeapon;
use crate::zero_hour::combat_import::DamageBonus;
use crate::zero_hour::combat_import::FALL;
use crate::zero_hour::combat_import::FALL_FRAMES;
use crate::zero_hour::combat_import::FiredProjectile;
use crate::zero_hour::combat_import::QUARTER_DEG;
use crate::zero_hour::combat_import::STAGES;
use crate::zero_hour::combat_import::StageModifier;
use crate::zero_hour::combat_import::Stages;
use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::damage_type::DamageType;
use crate::zero_hour::format::ini::weapon_ini::{IniWeapon, PrefireType, RadiusAffects, ShotDelay};
use crate::zero_hour::game_time;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::object_flight::ObjectFlight;
use crate::zero_hour::rules_names::RulesNames;
use crate::zero_hour::weapon_drop_cause::WeaponDropCause;
use crate::zero_hour::zero_hour_rules::ZeroHourRules;

/// An attack as the mode's data writes it, `[actions.weapon_<name>]` (09-zero-hour.md, A weapon
/// converts when it hits one target at once): its numbers, its clip, the projectile it fires,
/// the damage of its radius, its scatter, the pitches it reaches, and the speed of a weapon of
/// no projectile whose damage lands after its shot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportedWeapon {
    pub(crate) keys: ActionKeys,
    pub(crate) attack: AttackData,
    /// The projectile it fires, when it fires one.
    pub(crate) delivery: Option<ProjectileDeliveryData>,
}

impl ImportedWeapon {
    /// Its table under `[actions]`.
    pub(crate) fn table(&self) -> TomlTable {
        match &self.delivery {
            Some(delivery) => self.keys.table_delivering(&self.attack, delivery),
            None => self.keys.table(&self.attack),
        }
    }

    /// The attack `id` of `weapon`, by `rules`, the projectile it fires and the modifiers of its
    /// stages of continuous fire; why the import drops it, when it hits no target by the
    /// engine's rules, or holds a number the engine does not.
    pub(super) fn of<'a>(
        weapon: &IniWeapon,
        rules: &'a ZeroHourRules,
        id: &DeclaredName,
    ) -> Result<ConvertedWeapon<'a>, WeaponDropCause> {
        let object = match &weapon.projectile {
            Some(_) if weapon.stream => return Err(WeaponDropCause::Stream),
            Some(name) => Some(
                rules
                    .objects
                    .object(name.as_bytes())
                    .and_then(|object| Some((object, ObjectFlight::of(object, &rules.locomotors)?)))
                    .ok_or(WeaponDropCause::NoFlight)?,
            ),
            None => None,
        };
        if weapon.detonation_objects {
            return Err(WeaponDropCause::DetonationObjects);
        }
        let splashes = weapon.radius > 0.0 || weapon.secondary_radius > 0.0;
        if splashes && object.is_none() {
            return Err(WeaponDropCause::Radius);
        }
        let scatters = weapon.scatter > 0.0 || weapon.infantry_scatter > 0.0;
        if scatters && object.is_none() {
            return Err(WeaponDropCause::Scatter);
        }
        let affects = splashes
            .then(|| Self::relations(weapon.affects).ok_or(WeaponDropCause::Affects))
            .transpose()?;
        // A weapon's anti flags pick its targets: on the ground, the units that are no air
        // target, in the air, those that are (09-zero-hour.md, A weapon's anti flags are its
        // targets).
        let targets = match (weapon.anti_ground, weapon.anti_air) {
            (true, true) => &rules.names.enemies,
            (true, false) => &rules.names.enemies_on_ground,
            (false, true) => &rules.names.enemies_in_air,
            (false, false) => return Err(WeaponDropCause::NoTarget),
        };
        if weapon.affects.suicide {
            return Err(WeaponDropCause::Suicide);
        }
        if weapon.at_self {
            return Err(WeaponDropCause::AtSelf);
        }
        let stages = Self::stages(weapon, id, &rules.names)?;
        if weapon.upgrade.changes_other_fields() {
            return Err(WeaponDropCause::UpgradeBonus);
        }
        let bonus = Self::damage_bonus(weapon.upgrade.damage())?;
        // Each shot of a clip of one empties it, so no shot waits for its delay, and its period is
        // its reload (09-zero-hour.md, A clip of one shot is a clip).
        let period = match weapon.delay {
            _ if weapon.clip_size == 1 => weapon.clip_reload_ms,
            ShotDelay::Fixed(delay) => delay,
            ShotDelay::Random => return Err(WeaponDropCause::RandomDelay),
        };
        let period_ms = match game_time::of_ms(period)? {
            0 => return Err(WeaponDropCause::NoDelay),
            ms => ms,
        };
        let damage_kind = weapon.damage_type;
        if !damage_kind.takes_life() {
            return Err(WeaponDropCause::NoLife);
        }
        if damage_kind == DamageType::Healing {
            return Err(WeaponDropCause::Healing);
        }
        let number = |value: f32| {
            let value = game_float::to_num(value).map_err(|_| WeaponDropCause::BadNumber)?;
            if value < Num::ZERO {
                return Err(WeaponDropCause::BadNumber);
            }
            Ok(value)
        };
        let ms = |ms: u64| {
            i64::try_from(ms)
                .map(Self::own)
                .map_err(|_| WeaponDropCause::BadNumber)
        };
        // A pre-attack of 0 ms waits for nothing, as `Weapon::preFireWeapon` sets none
        // (04-capabilities/combat.md, A pre-attack).
        let pre_attack = match game_time::of_ms(weapon.pre_attack_ms)? {
            0 => None,
            pre_ms => Some(PreAttackData {
                ms: ms(pre_ms)?,
                each: match weapon.pre_attack_type {
                    PrefireType::Shot => PreAttackEach::Shot,
                    PrefireType::Attack => PreAttackEach::Target,
                    PrefireType::Clip => PreAttackEach::Clip,
                },
            }),
        };
        let clip = match weapon.clip_size {
            ..0 => return Err(WeaponDropCause::BadNumber),
            0 => None,
            size => Some(ClipData {
                shots: Self::own(i64::from(size)),
                reload_ms: ms(game_time::of_ms(weapon.clip_reload_ms)?)?,
                refill: if weapon.reloads_at_home {
                    ClipRefill::Home
                } else {
                    ClipRefill::Time
                },
            }),
        };
        // Damage within a secondary radius past the primary is an outer ring; one no farther
        // than the primary deals nothing beyond it, as each unit within the primary takes the
        // primary damage alone (`dealDamageInternal`).
        let splash = match affects {
            Some(affects) => {
                let radius = number(weapon.radius)?;
                let splash = SplashData::new(radius, affects, !weapon.affects.source)
                    .ok_or(WeaponDropCause::BadNumber)?;
                if weapon.secondary_radius > weapon.radius {
                    let amount = Number::Value(Scalar::Decimal(number(weapon.secondary_damage)?));
                    let outer =
                        OuterData::new(number(weapon.secondary_radius)?, Ranked::One(amount));
                    Some(splash.with_outer(outer).ok_or(WeaponDropCause::BadNumber)?)
                } else {
                    Some(splash)
                }
            }
            None => None,
        };
        let scatter = if scatters {
            let mut tags = BTreeMap::new();
            if weapon.infantry_scatter > 0.0 {
                let infantry = rules.names.infantry.clone();
                tags.insert(infantry, number(weapon.infantry_scatter)?);
            }
            let scatter = ScatterData::new(number(weapon.scatter)?, tags);
            Some(scatter.ok_or(WeaponDropCause::BadNumber)?)
        } else {
            None
        };
        // The original takes a bound of its defaults, a turn either way, as none.
        let [low, high] = weapon.pitch;
        if !(low.is_finite() && high.is_finite()) {
            return Err(WeaponDropCause::BadNumber);
        }
        let pitch_deg = if low > -180.0 || high < 180.0 {
            let [low, high] = [low.max(-QUARTER_DEG), high.min(QUARTER_DEG)];
            let pitch =
                |value: f32| game_float::to_num(value).map_err(|_| WeaponDropCause::BadNumber);
            Some(PitchData::new(pitch(low)?, pitch(high)?).ok_or(WeaponDropCause::BadNumber)?)
        } else {
            None
        };
        // A laser's damage lands at once, and a projectile's when it ends.
        let speed = if object.is_none() && !weapon.laser && weapon.speed < IniWeapon::INSTANT {
            Some(Speed::new(number(weapon.speed)?).ok_or(WeaponDropCause::BadNumber)?)
        } else {
            None
        };
        let fired = match object {
            Some((object, flight)) => Some(FiredProjectile {
                object,
                projectile: flight.projectile(weapon, object, &rules.names)?,
            }),
            None => None,
        };
        let damage = Ranked::One(Number::Value(Scalar::Decimal(number(weapon.damage)?)));
        let meters = number(weapon.range)?;
        let range = RangeField::Range(ActionRange::meters(meters));
        let min_range = if weapon.min_range > 0.0 {
            let min = number(weapon.min_range)?;
            if min >= meters {
                return Err(WeaponDropCause::MinRange);
            }
            Some(Ranked::One(Number::Value(Scalar::Decimal(min))))
        } else {
            None
        };
        // Its attacker turns toward its target until it lies within its acceptable delta, of at
        // least 0.035 radians, as `AIAttackAimAtTargetState::update` aims; a delta of a half turn
        // needs no turn.
        let aim_deg = if weapon.aim_delta < 180.0 {
            let delta = number(weapon.aim_delta)?;
            Some(delta.max(Self::least_aim()))
        } else {
            None
        };
        let targets = targets.clone();
        Ok(ConvertedWeapon {
            weapon: Self {
                keys: ActionKeys {
                    range: Some(Ranked::One(range)),
                    min_range,
                    ..ActionKeys::new(Targeting::Unit(targets))
                },
                attack: AttackData {
                    rate: WeaponValue::Own(ms(period_ms)?),
                    rate_bonus: Some(Self::rate_bonus(&rules.names, stages.is_some())),
                    damage: WeaponValue::Own(damage),
                    damage_bonus: Some(Self::damage_bonus_stats(&rules.names, bonus.as_ref())),
                    range_bonus: Some(Self::range_bonus(&rules.names)),
                    damage_kind: damage_kind.kind(),
                    prefers: None,
                    clip,
                    splash,
                    scatter,
                    pitch_deg,
                    speed,
                    streak: stages.as_ref().map(|stages| stages.streak.clone()),
                    pre_attack,
                    aim_deg,
                },
                delivery: None,
            },
            fired,
            modifiers: stages.map_or_else(Vec::new, |stages| stages.modifiers),
            bonus,
        })
    }

    /// The least aim of a weapon, `AIAttackAimAtTargetState`'s 0.035 radians, in degrees.
    pub(super) fn least_aim() -> Num {
        let radians: Num = "0.035".parse().expect("a decimal");
        radians
            .checked_mul_div(Num::int(180), Num::PI, Rounding::NearestEven)
            .expect("a small angle in degrees fits")
    }

    /// The stats a weapon's rate of fire takes: the veterancy's, the propaganda towers', and the
    /// stages' of a weapon of continuous fire, as `Weapon::computeBonus` sums each condition's
    /// bonus (04-capabilities/combat.md, A weapon's bonus is a sum of stats).
    fn rate_bonus(names: &RulesNames, staged: bool) -> BonusData {
        let stage = staged.then(|| names.rate_of_fire.clone());
        let every = [names.veterancy_rate.clone(), names.propaganda_rate.clone()];
        let stats = every.into_iter().chain(stage);
        BonusData::of(stats).expect("the veterancy's stat at least")
    }

    /// The stats a weapon's range takes: the garrison's.
    fn range_bonus(names: &RulesNames) -> BonusData {
        BonusData::of([names.garrison_range.clone()]).expect("the garrison's stat")
    }

    /// The stats a weapon's damage takes: the veterancy's, the garrison's, and its upgrade's
    /// `bonus`.
    fn damage_bonus_stats(names: &RulesNames, bonus: Option<&DamageBonus>) -> BonusData {
        let upgrade = bonus.map(|bonus| bonus.stat.clone());
        let fixed = [
            names.veterancy_damage.clone(),
            names.garrison_damage.clone(),
        ];
        let stats = fixed.into_iter().chain(upgrade);
        BonusData::of(stats).expect("the veterancy's stat at least")
    }

    /// The damage bonus of `factor`, a weapon's of `PLAYER_UPGRADE`: none for a factor of 1, which
    /// changes nothing; a bad number for a percent below 0, past a `u32` or not finite.
    fn damage_bonus(factor: f32) -> Result<Option<DamageBonus>, WeaponDropCause> {
        if factor == 1.0 {
            return Ok(None);
        }
        let percent = (factor * 100.0).round();
        if !(0.0..4_294_967_296.0).contains(&f64::from(percent)) {
            return Err(WeaponDropCause::BadNumber);
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a whole percent within a u32"
        )]
        let percent = percent as u32;
        let stat = Stat::named(&format!("upgrade_damage_{percent}")).expect("a stat's name");
        let added = game_float::to_num(factor - 1.0).map_err(|_| WeaponDropCause::BadNumber)?;
        Ok(Some(DamageBonus {
            stat,
            added: Number::Value(Scalar::Decimal(added)),
        }))
    }

    /// The streak of `weapon`'s continuous fire, of the action `id`, and the modifiers of its two
    /// stages, named after it, above `ContinuousFireOne` and `ContinuousFireTwo` shots, each of
    /// its stage's rate of fire, which adds 0 for a stage of no bonus; none for a weapon whose
    /// stages change no rate, as they change nothing. Dropped for a stage's bonus to another
    /// field, or a coast of no time, which the engine does not hold, and for shots below 0 or
    /// not growing (09-zero-hour.md, Continuous fire converts).
    fn stages(
        weapon: &IniWeapon,
        id: &DeclaredName,
        names: &RulesNames,
    ) -> Result<Option<Stages>, WeaponDropCause> {
        let fire = weapon.continuous_fire;
        if fire.changes_other_fields() {
            return Err(WeaponDropCause::ContinuousFire);
        }
        let rates = fire.rates();
        if rates == [1.0; 2] {
            return Ok(None);
        }
        let [one, two] = fire
            .shots
            .map(|shots| u32::try_from(shots).map_err(|_| WeaponDropCause::BadNumber));
        let shots = [one?, two?];
        if shots[0] >= shots[1] {
            return Err(WeaponDropCause::BadNumber);
        }
        let coast_ms = match game_time::of_ms(fire.coast_ms)? {
            0 => return Err(WeaponDropCause::ContinuousFire),
            ms => ms,
        };
        let named = |suffix| {
            ImportName::of(id.as_str().as_bytes())
                .suffixed(suffix)
                .map_err(|_| WeaponDropCause::LongName)
        };
        let mut modifiers = Vec::with_capacity(STAGES.len() + 1);
        let mut stages = Vec::with_capacity(STAGES.len());
        let staged = STAGES.into_iter().zip(&names.continuous_fire);
        for (((stage, tag), shots), rate) in staged.zip(shots).zip(rates) {
            let name = named(stage)?;
            // `WeaponBonus::appendBonuses` adds a bonus less 1, in the game's `f32`.
            let added = game_float::to_num(rate - 1.0).map_err(|_| WeaponDropCause::BadNumber)?;
            let change = Number::Value(Scalar::Decimal(added));
            modifiers.push((
                name.clone(),
                StageModifier {
                    stats: BTreeMap::from([(names.rate_of_fire.clone(), change)]),
                    tags: vec![tag.clone()],
                },
            ));
            stages.push(StageData {
                shots,
                modifier: name,
            });
        }
        let fall = named(FALL)?;
        modifiers.push((
            fall.clone(),
            StageModifier {
                stats: BTreeMap::new(),
                tags: vec![names.continuous_fire_slow.clone()],
            },
        ));
        let fall = FallData {
            modifier: fall,
            ms: game_time::frames_ms(FALL_FRAMES),
        };
        Ok(Some(Stages {
            streak: StreakData {
                coast_ms,
                stages,
                fall: Some(fall),
            },
            modifiers,
        }))
    }

    /// `value` at every rank.
    pub(super) fn own(value: i64) -> Ranked<Number> {
        Ranked::One(Number::from(value))
    }

    /// The relations of `affects`, a projectile's splash, as a splash's filter names them, each
    /// player a team of its own: of `ALLIES`, `ENEMIES` and `NEUTRALS` all, enemies and neutrals,
    /// or one alone; none for another set, and for a radius that spares the airborne. `NOT_SIMILAR`
    /// changes nothing: a projectile is its blast's source, so it spares only allied projectiles of
    /// its type, which take no damage (09-zero-hour.md, `NOT_SIMILAR` of a projectile's blast
    /// converts to nothing).
    pub(super) const fn relations(affects: RadiusAffects) -> Option<RelationSet> {
        if affects.not_airborne {
            return None;
        }
        match (affects.allies, affects.enemies, affects.neutrals) {
            (true, true, true) => Some(RelationSet::All),
            (false, true, true) => Some(RelationSet::Enemies),
            (false, true, false) => Some(RelationSet::Hostiles),
            (false, false, true) => Some(RelationSet::Neutrals),
            (true, false, false) => Some(RelationSet::Allies),
            _ => None,
        }
    }
}
