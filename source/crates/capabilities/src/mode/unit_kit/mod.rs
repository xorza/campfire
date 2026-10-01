use campfire_math::Num;
use campfire_sim::TickRate;

use crate::combat::combat_data::CombatData;
use crate::combat::on_death::OnDeath;
use crate::mode::error::UnitKitError;
use crate::navigation::move_step::MoveStep;
use crate::progression::track_set::TrackSet;
use crate::stats::pool_id::PoolId;
use crate::stats::pools::Pools;
use crate::stats::stat::{EngineStat, Stat};
use crate::stats::stats_data::StatsData;
use crate::units::body::Body;
use crate::values::speed::Speed;
use crate::vision::sight::Sight;
use crate::vision::vision_data::VisionData;

/// What a new unit of a type starts with, in ticks at the match's rate: its pools, full at their
/// maxima at level 1, whether it stays when it dies, when it has a `combat` section, how far it
/// walks a tick, how far it sees, its body, and the tracks it gains experience on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitKit {
    pub pools: Option<Pools>,
    pub on_death: Option<OnDeath>,
    pub step: Option<MoveStep>,
    pub sight: Option<Sight>,
    pub body: Option<Body>,
    pub tracks: TrackSet,
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
    /// pool. Its move speed is capped at the rules' cap.
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
        let on_death = combat
            .map(|combat| {
                if pools.is_none_or(|pools| pools.max(rules.life).is_none()) {
                    return Err(UnitKitError::NoLifePool);
                }
                Ok(combat.on_death)
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
            on_death,
            step,
            sight: None,
            body: None,
            tracks: TrackSet::default(),
        })
    }

    /// The kit with `body`, the body of its type's `collision` section on its layer, if it has
    /// one.
    #[must_use]
    pub const fn with_body(self, body: Option<Body>) -> UnitKit {
        UnitKit { body, ..self }
    }

    /// The kit with `tracks`, its type's.
    #[must_use]
    pub const fn with_tracks(self, tracks: TrackSet) -> UnitKit {
        UnitKit { tracks, ..self }
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

/// `speed` in meters a second as meters a tick at `rate`, to the nearest.
const fn per_tick(speed: Num, rate: TickRate) -> Option<Num> {
    speed.checked_div_int(rate.hz().get() as i64)
}

#[cfg(test)]
mod tests;
