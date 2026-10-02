use std::num::NonZeroU8;

use campfire_math::Num;
use campfire_sim::TickRate;

use crate::combat::combat_data::CombatData;
use crate::combat::on_death::OnDeath;
use crate::mode::error::UnitKitError;
use crate::production::production_data::ProductionData;
use crate::progression::track_set::TrackSet;
use crate::stats::move_step::MoveStep;
use crate::stats::pool_id::PoolId;
use crate::stats::pools::Pools;
use crate::stats::stat_book::StatBook;
use crate::units::body::Body;
use crate::units::unit_type::UnitType;
use crate::values::stat::{EngineStat, Stat};
use crate::vision::sight::Sight;
use crate::vision::vision_data::VisionData;

/// What a new unit of a type starts with, in ticks at the match's rate: its pools, full at their
/// maxima at level 1, whether it stays when it dies, when it has a `combat` section, how far it
/// walks a tick, how far it sees, its body, the tracks it gains experience on, and the most
/// trains its queue holds, when it has a `production` section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitKit {
    pub pools: Option<Pools>,
    pub on_death: Option<OnDeath>,
    pub step: Option<MoveStep>,
    pub sight: Option<Sight>,
    pub body: Option<Body>,
    pub tracks: TrackSet,
    pub queue: Option<NonZeroU8>,
}

impl UnitKit {
    /// The kit of `unit_type`, of the stats `book` gives it, with `combat` and `pools`, each
    /// with the stat of its maximum. Each pool's maximum is that stat at level 1, and its move
    /// step is the book's at level 1 at `rate`, as a refresh computes them. A type with `combat`
    /// has the life pool `life`, which a mode with no combat lacks.
    pub(crate) fn new<'a>(
        book: &StatBook,
        unit_type: UnitType,
        combat: Option<&CombatData>,
        pools: impl IntoIterator<Item = (PoolId, &'a Stat)>,
        life: Option<PoolId>,
        rate: TickRate,
    ) -> Result<UnitKit, UnitKitError> {
        let values = book.base_values(unit_type, 1);
        let given = |stat: &Stat| {
            let id = book.named(stat).expect("the load checked the stats");
            book.gives(unit_type, id).then_some(id)
        };
        let mut maxes = Vec::new();
        for (pool, max) in pools {
            let id = given(max).ok_or_else(|| UnitKitError::MissingStat(max.clone()))?;
            let value = values[id.index()];
            if value <= Num::ZERO {
                return Err(UnitKitError::NotPositive(max.clone()));
            }
            maxes.push((pool, value));
        }
        let pools =
            (!maxes.is_empty()).then(|| Pools::new(maxes).expect("each maximum is positive"));
        let on_death = combat
            .map(|combat| {
                let life = life.and_then(|life| pools?.max(life));
                if life.is_none() {
                    return Err(UnitKitError::NoLifePool);
                }
                Ok(combat.on_death)
            })
            .transpose()?;
        let step = given(&Stat::Engine(EngineStat::MoveSpeed)).map(|_| {
            let step = book
                .step(&values, rate)
                .expect("a type gives only a stat the mode declares");
            MoveStep::new(step).expect("the book's step is never negative")
        });
        Ok(UnitKit {
            pools,
            on_death,
            step,
            sight: None,
            body: None,
            tracks: TrackSet::default(),
            queue: None,
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

    /// The kit with the train queue of its type's `production` section, if it has one.
    #[must_use]
    pub fn with_production(self, production: Option<&ProductionData>) -> UnitKit {
        UnitKit {
            queue: production.map(|production| production.queue),
            ..self
        }
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

#[cfg(test)]
mod tests;
