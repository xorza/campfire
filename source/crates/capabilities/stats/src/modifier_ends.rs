use bevy_ecs::entity::Entity;
use bevy_ecs::query::Added;
use bevy_ecs::system::{Local, Query, Res};
use campfire_sim::{DueUnit, SimTick, StableId};
use campfire_units::Dead;

use crate::carried_mut::CarriedMut;
use crate::modifier_dues::ModifierIndexes;
use crate::modifiers::Modifiers;
use crate::modifiers::modifier_clocks::ModifierClocks;

/// The ends of modifiers: those that hold no longer, and those of the units that died.
#[derive(Debug)]
pub struct ModifierEnds;

impl ModifierEnds {
    /// Ends, as each tick starts, the modifiers and stacks that hold no longer, of the units the
    /// index of the ends holds due.
    pub(super) fn expire_modifiers(
        tick: Res<'_, SimTick>,
        mut units: Query<'_, '_, (&mut Modifiers, &mut ModifierClocks)>,
        mut indexes: ModifierIndexes<'_>,
        mut due: Local<'_, Vec<DueUnit>>,
    ) {
        let now = tick.start();
        due.clear();
        indexes.take_ends(now, &mut due);
        for &DueUnit { id, entity } in &*due {
            if let Ok((modifiers, clocks)) = units.get_mut(entity) {
                CarriedMut::new(modifiers, clocks, indexes.of(id, entity)).expire(now);
            }
        }
    }

    /// Ends the modifiers of each unit that died this tick, all but its passives.
    pub(super) fn clear_dead_modifiers(
        mut dead: Query<
            '_,
            '_,
            (Entity, &StableId, &mut Modifiers, &mut ModifierClocks),
            Added<Dead>,
        >,
        mut indexes: ModifierIndexes<'_>,
    ) {
        for (entity, &id, modifiers, clocks) in &mut dead {
            CarriedMut::new(modifiers, clocks, indexes.of(id, entity)).clear_on_death();
        }
    }
}
