use bevy_ecs::change_detection::Mut;
use bevy_ecs::entity::Entity;
use bevy_ecs::system::{ResMut, SystemParam};
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_sim::{DueIndex, DueUnit, StableId};

use crate::modifiers::Modifiers;
use crate::modifiers::modifier_clocks::ModifierClocks;

/// The indexes of the modifiers' ends and intervals, as a system that changes a unit's modifiers
/// takes them.
#[derive(SystemParam, Debug)]
pub(crate) struct ModifierIndexes<'w> {
    ends: ResMut<'w, DueIndex<Modifiers>>,
    intervals: ResMut<'w, DueIndex<ModifierClocks>>,
}

/// The indexes of the modifiers of the unit `id`, of `entity`, which each change of its
/// modifiers or their clocks pushes it into.
#[derive(Debug)]
pub(crate) struct ModifierDues<'a> {
    ends: &'a mut DueIndex<Modifiers>,
    intervals: &'a mut DueIndex<ModifierClocks>,
    id: StableId,
    entity: Entity,
}

impl ModifierIndexes<'_> {
    /// Moves each unit whose modifiers end at `now` or before into `due`, out of the index of the
    /// ends, by tick, then stable id.
    pub(crate) fn take_ends(&mut self, now: Tick, due: &mut Vec<DueUnit>) {
        self.ends.take_due(now, due);
    }

    /// Moves each unit whose interval comes at `now` or before into `due`, out of the index of
    /// the intervals, by tick, then stable id.
    pub(crate) fn take_intervals(&mut self, now: Tick, due: &mut Vec<DueUnit>) {
        self.intervals.take_due(now, due);
    }

    /// The indexes, for the modifiers of the unit `id`, of `entity`.
    pub(crate) fn of(&mut self, id: StableId, entity: Entity) -> ModifierDues<'_> {
        ModifierDues {
            ends: &mut self.ends,
            intervals: &mut self.intervals,
            id,
            entity,
        }
    }
}

impl ModifierDues<'_> {
    /// Pushes the unit as its `modifiers` and their `clocks` make it due: at their first end,
    /// and at their first interval.
    pub(crate) fn push(&mut self, modifiers: &Modifiers, clocks: &ModifierClocks) {
        if let Some(at) = modifiers.next_end() {
            self.ends.push(at, self.id, self.entity);
        }
        if let Some(at) = clocks.next_interval() {
            self.intervals.push(at, self.id, self.entity);
        }
    }

    /// The same indexes, for a shorter borrow.
    pub(crate) const fn reborrow(&mut self) -> ModifierDues<'_> {
        ModifierDues {
            ends: self.ends,
            intervals: self.intervals,
            id: self.id,
            entity: self.entity,
        }
    }

    /// Runs `change` on `world`, with the indexes the modifiers of the unit `id`, of `entity`,
    /// push it into, as an exclusive system or an effect changes the unit.
    pub(crate) fn with<R>(
        world: &mut World,
        id: StableId,
        entity: Entity,
        change: impl FnOnce(&mut World, &mut ModifierDues<'_>) -> R,
    ) -> R {
        world.resource_scope(|world, mut ends: Mut<'_, DueIndex<Modifiers>>| {
            world.resource_scope(|world, mut intervals: Mut<'_, DueIndex<ModifierClocks>>| {
                let mut dues = ModifierDues {
                    ends: &mut ends,
                    intervals: &mut intervals,
                    id,
                    entity,
                };
                change(world, &mut dues)
            })
        })
    }
}
