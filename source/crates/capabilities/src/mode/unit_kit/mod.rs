use campfire_math::Num;
use campfire_sim::{TickRate, Ticks};

use crate::combat::attack_stats::AttackStats;
use crate::combat::combat_data::{AttackData, CombatData};
use crate::combat::combatant::Combatant;
use crate::combat::health::Health;
use crate::mode::error::UnitKitError;
use crate::navigation::move_step::MoveStep;
use crate::stats::stat::EngineStat;
use crate::stats::stats_data::StatsData;
use crate::values::speed::Speed;
use crate::vision::sight::Sight;
use crate::vision::vision_data::VisionData;

/// The most attacks a unit makes a second.
const MAX_ATTACK_SPEED: Num = Num::from_bits(5 << (Num::FRAC_BITS - 1));

/// What a new unit of a type starts with, in ticks at the match's rate: its combat values from its
/// `combat` section and its stats at level 1, how far it walks a tick, and how far it sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitKit {
    pub combatant: Option<Combatant>,
    pub step: Option<MoveStep>,
    pub sight: Option<Sight>,
}

/// The match's rules a unit type's values meet: its tick rate, and the mode's move speed cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KitRules {
    pub rate: TickRate,
    pub max_move_speed: Speed,
}

impl UnitKit {
    /// The kit of a type with `stats` and `combat`. Its health is its `health`; its attack deals
    /// `attack_damage` and starts at most `attack_speed` times a second, at most 2.5, so its
    /// period is the rate over that, rounded up. Its move speed is capped at the rules' cap. A
    /// ranged attack's projectile homes, so it must fly faster than the cap, in the steps the
    /// match takes.
    pub fn new(
        stats: Option<&StatsData>,
        combat: Option<&CombatData>,
        rules: KitRules,
    ) -> Result<UnitKit, UnitKitError> {
        let stat = |stat| {
            let stats = stats.ok_or(UnitKitError::MissingStat(stat))?;
            if !stats.declares(stat) {
                return Err(UnitKitError::MissingStat(stat));
            }
            stats.at(stat, 1).ok_or(UnitKitError::Overflow(stat))
        };
        let combatant = combat
            .map(|combat| {
                let health = Health::new(stat(EngineStat::Health)?)
                    .ok_or(UnitKitError::NotPositive(EngineStat::Health))?;
                let attack = combat
                    .attack
                    .map(|attack| attack_stats(&attack, stat, rules))
                    .transpose()?;
                Ok(Combatant {
                    health,
                    attack,
                    on_death: combat.on_death,
                })
            })
            .transpose()?;
        let step = if stats.is_some_and(|stats| stats.declares(EngineStat::MoveSpeed)) {
            let speed = stat(EngineStat::MoveSpeed)?.min(rules.max_move_speed.get());
            let step =
                per_tick(speed, rules.rate).ok_or(UnitKitError::Overflow(EngineStat::MoveSpeed))?;
            Some(MoveStep::new(step).ok_or(UnitKitError::Negative(EngineStat::MoveSpeed))?)
        } else {
            None
        };
        Ok(UnitKit {
            combatant,
            step,
            sight: None,
        })
    }

    /// The kit with the sight of its type's `vision` section, if it has one.
    #[must_use]
    pub fn with_vision(self, vision: Option<&VisionData>) -> UnitKit {
        UnitKit {
            sight: vision.map(|vision| vision.sight),
            ..self
        }
    }
}

/// The attack of `attack`, with the stats `stat` reads.
fn attack_stats(
    attack: &AttackData,
    stat: impl Fn(EngineStat) -> Result<Num, UnitKitError>,
    rules: KitRules,
) -> Result<AttackStats, UnitKitError> {
    let hz = rules.rate.hz().get();
    let speed = stat(EngineStat::AttackSpeed)?.min(MAX_ATTACK_SPEED);
    if speed <= Num::ZERO {
        return Err(UnitKitError::NotPositive(EngineStat::AttackSpeed));
    }
    // hz ÷ speed in ticks, rounded up: exact, as the speed's bits count 2⁻²⁴ attacks a second.
    let bits = u128::try_from(speed.to_bits()).expect("a positive speed");
    let period = (u128::from(hz) << Num::FRAC_BITS).div_ceil(bits);
    let period = u64::try_from(period)
        .ok()
        .map(Ticks::new)
        .ok_or(UnitKitError::Overflow(EngineStat::AttackSpeed))?;
    let windup = rules
        .rate
        .ticks(attack.windup_ms)
        .ok_or(UnitKitError::TimeTooLarge)?;
    let range = attack.range.to_num().ok_or(UnitKitError::Range)?;
    let melee = AttackStats::new(range, windup, period, stat(EngineStat::AttackDamage)?)
        .ok_or(UnitKitError::Attack)?;
    let Some(speed) = attack.projectile_speed else {
        return Ok(melee);
    };
    let speed = speed.to_num().and_then(|speed| per_tick(speed, rules.rate));
    let cap = per_tick(rules.max_move_speed.get(), rules.rate);
    match (speed, cap) {
        (Some(speed), Some(cap)) if speed > cap => Ok(melee
            .ranged(speed)
            .expect("faster than a cap that is not negative")),
        _ => Err(UnitKitError::ProjectileNotFaster),
    }
}

/// `speed` in meters a second as meters a tick at `rate`, to the nearest.
const fn per_tick(speed: Num, rate: TickRate) -> Option<Num> {
    speed.checked_div_int(rate.hz().get() as i64)
}

#[cfg(test)]
mod tests;
