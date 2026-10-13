use bevy_ecs::query::Added;
use bevy_ecs::system::{Query, Res};
use campfire_core::Dead;
use campfire_sim::SimTick;

use crate::carried_mut::CarriedMut;
use crate::modifiers::Modifiers;
use crate::modifiers::modifier_clocks::ModifierClocks;

/// The ends of modifiers: those that hold no longer, and those of the units that died.
#[derive(Debug)]
pub struct ModifierEnds;

impl ModifierEnds {
    /// Ends, as each tick starts, the modifiers and stacks that hold no longer.
    pub(super) fn expire_modifiers(
        tick: Res<'_, SimTick>,
        mut units: Query<'_, '_, (&mut Modifiers, &mut ModifierClocks)>,
    ) {
        let now = tick.start();
        for (modifiers, clocks) in &mut units {
            CarriedMut::new(modifiers, clocks).expire(now);
        }
    }

    /// Ends the modifiers of each unit that died this tick, all but its passives.
    pub(super) fn clear_dead_modifiers(
        mut dead: Query<'_, '_, (&mut Modifiers, &mut ModifierClocks), Added<Dead>>,
    ) {
        for (modifiers, clocks) in &mut dead {
            CarriedMut::new(modifiers, clocks).clear_on_death();
        }
    }
}
