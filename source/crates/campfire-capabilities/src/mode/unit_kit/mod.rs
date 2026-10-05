use std::num::NonZeroU8;

use campfire_math::Num;
use campfire_sim::TickRate;

use crate::actions::slot_kind::SlotKind;
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
/// walks a tick, how far it sees, its body, the tracks it gains experience on, the most trains
/// its queue holds, when it has a `production` section, and its inventory's slots and slot kind,
/// when it has one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitKit {
    pub pools: Option<Pools>,
    pub on_death: Option<OnDeath>,
    pub step: Option<MoveStep>,
    pub sight: Option<Sight>,
    pub body: Option<Body>,
    pub tracks: TrackSet,
    pub queue: Option<NonZeroU8>,
    pub inventory: Option<InventorySpec>,
}

/// An inventory as a unit type gives it: how many slots, and the slot kind their actions fill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InventorySpec {
    pub slots: NonZeroU8,
    pub kind: SlotKind,
}

/// What a unit type's file gives its kit: its `combat` section, its pools, each with the stat of
/// its maximum, its `vision` section, its body, its tracks, its `production` section and its
/// inventory.
#[derive(Debug, Clone)]
pub(crate) struct KitSections<'a, P> {
    pub(crate) combat: Option<&'a CombatData>,
    pub(crate) pools: P,
    pub(crate) vision: Option<&'a VisionData>,
    pub(crate) body: Option<Body>,
    pub(crate) tracks: TrackSet,
    pub(crate) production: Option<&'a ProductionData>,
    pub(crate) inventory: Option<InventorySpec>,
}

impl UnitKit {
    /// The kit of `unit_type`, of the stats `book` gives it and of its `sections`. Each pool's
    /// maximum is the stat of its maximum at level 1, and the move step is the book's at level 1
    /// at `rate`, as a refresh computes them. A type has `combat` exactly when it has the life
    /// pool `life`, which a mode with no combat lacks: a unit that can die is one that combat
    /// kills.
    pub(crate) fn new<'a>(
        book: &StatBook,
        unit_type: UnitType,
        sections: KitSections<'a, impl IntoIterator<Item = (PoolId, &'a Stat)>>,
        life: Option<PoolId>,
        rate: TickRate,
    ) -> Result<UnitKit, UnitKitError> {
        let KitSections {
            combat,
            pools,
            vision,
            body,
            tracks,
            production,
            inventory,
        } = sections;
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
        let has_life = life.and_then(|life| pools?.max(life)).is_some();
        let on_death = match (combat, has_life) {
            (Some(combat), true) => Some(combat.on_death),
            (Some(_), false) => return Err(UnitKitError::NoLifePool),
            (None, true) => return Err(UnitKitError::NoCombat),
            (None, false) => None,
        };
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
            sight: vision.map(|vision| vision.sight),
            body,
            tracks,
            queue: production.map(|production| production.queue),
            inventory,
        })
    }
}

#[cfg(test)]
mod tests;
