use campfire_math::Num;
use campfire_sim::{TickRate, Ticks};

use crate::combat::attack_stats::AttackStats;
use crate::combat::combat_data::{AttackData, CombatData};
use crate::combat::combatant::Combatant;
use crate::mode::error::UnitKitError;
use crate::navigation::move_step::MoveStep;
use crate::stats::pool_id::PoolId;
use crate::stats::pools::Pools;
use crate::stats::stat::{EngineStat, Stat};
use crate::stats::stats_data::StatsData;
use crate::units::body::Body;
use crate::values::speed::Speed;
use crate::vision::sight::Sight;
use crate::vision::vision_data::VisionData;

/// What a new unit of a type starts with, in ticks at the match's rate: its pools, full at their
/// maxima at level 1, its combat values from its `combat` section and its stats at level 1, how
/// far it walks a tick, how far it sees, and its body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitKit {
    pub pools: Option<Pools>,
    pub combatant: Option<Combatant>,
    pub step: Option<MoveStep>,
    pub sight: Option<Sight>,
    pub body: Option<Body>,
}

/// The match's rules a unit type's values meet: its tick rate, the mode's move speed cap, and
/// its life pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KitRules {
    pub rate: TickRate,
    pub max_move_speed: Speed,
    pub life: PoolId,
}

impl UnitKit {
    /// The kit of a type with `stats`, `combat`, and `pools`, each with the stat of its maximum.
    /// Each pool's maximum is that stat at level 1, and a type with `combat` has the rules' life
    /// pool. Its attack deals `attack_damage` and starts at most `attack_speed` times a second,
    /// so its period is the rate over that, rounded up. Its move speed is capped at the rules'
    /// cap. A ranged attack's projectile homes, so it must fly faster than the cap, in the steps
    /// the match takes.
    pub fn new<'a>(
        stats: Option<&StatsData>,
        combat: Option<&CombatData>,
        pools: impl IntoIterator<Item = (PoolId, &'a Stat)>,
        rules: KitRules,
    ) -> Result<UnitKit, UnitKitError> {
        let stat = |stat: &Stat| {
            let missing = || UnitKitError::MissingStat(stat.clone());
            let stats = stats.ok_or_else(missing)?;
            if !stats.declares(stat) {
                return Err(missing());
            }
            stats
                .at(stat, 1)
                .ok_or_else(|| UnitKitError::Overflow(stat.clone()))
        };
        let mut maxes = Vec::new();
        for (pool, max) in pools {
            let value = stat(max)?;
            if value <= Num::ZERO {
                return Err(UnitKitError::NotPositive(max.clone()));
            }
            maxes.push((pool, value));
        }
        let pools =
            (!maxes.is_empty()).then(|| Pools::new(maxes).expect("each maximum is positive"));
        let combatant = combat
            .map(|combat| {
                if pools.is_none_or(|pools| pools.max(rules.life).is_none()) {
                    return Err(UnitKitError::NoLifePool);
                }
                let attack = combat
                    .attack
                    .map(|attack| attack_stats(&attack, stat, rules))
                    .transpose()?;
                Ok(Combatant {
                    attack,
                    on_death: combat.on_death,
                })
            })
            .transpose()?;
        let move_speed = Stat::Engine(EngineStat::MoveSpeed);
        let step = if stats.is_some_and(|stats| stats.declares(&move_speed)) {
            let speed = stat(&move_speed)?.min(rules.max_move_speed.get());
            let overflow = || UnitKitError::Overflow(move_speed.clone());
            let step = per_tick(speed, rules.rate).ok_or_else(overflow)?;
            Some(MoveStep::new(step).ok_or(UnitKitError::Negative(move_speed))?)
        } else {
            None
        };
        Ok(UnitKit {
            pools,
            combatant,
            step,
            sight: None,
            body: None,
        })
    }

    /// The kit with `body`, the body of its type's `collision` section on its layer, if it has
    /// one.
    #[must_use]
    pub const fn with_body(self, body: Option<Body>) -> UnitKit {
        UnitKit { body, ..self }
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
    stat: impl Fn(&Stat) -> Result<Num, UnitKitError>,
    rules: KitRules,
) -> Result<AttackStats, UnitKitError> {
    let attack_speed = Stat::Engine(EngineStat::AttackSpeed);
    let speed = stat(&attack_speed)?;
    if speed <= Num::ZERO {
        return Err(UnitKitError::NotPositive(attack_speed));
    }
    let windup = rules
        .rate
        .ticks(attack.windup_ms)
        .ok_or(UnitKitError::TimeTooLarge)?;
    // A windup not shorter than the period at level 1 is a mistake of the data; a period that
    // later shrinks past it, as attack speed grows, the derived stats stretch.
    let attacks = u128::from(speed.to_bits().unsigned_abs()) << Num::FRAC_BITS;
    let period = AttackStats::period_at(rules.rate.hz().get(), attacks, Ticks::ZERO);
    let range = attack.range.to_num().ok_or(UnitKitError::Range)?;
    let damage = stat(&Stat::Engine(EngineStat::AttackDamage))?;
    let melee = AttackStats::new(range, windup, period, damage).ok_or(UnitKitError::Attack)?;
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
