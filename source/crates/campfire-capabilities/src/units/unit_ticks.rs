use bevy_ecs::entity::Entity;
use bevy_ecs::system::{Commands, NonSendMut, Query, Res, ResMut};
use campfire_sim::SimTick;

use crate::scripts::script_budgets::ScriptBudgets;
use crate::scripts::script_failures::ScriptFailures;
use crate::units::lifespan::Lifespan;
use crate::units::new_unit_states::NewUnitStates;

/// The core's systems that bound each tick: the state of one tick cleared as it starts, and the
/// timed lives that end with it.
#[derive(Debug)]
pub(super) struct UnitTicks;

impl UnitTicks {
    /// Clears, as each tick starts, the script budgets' use, the failures recorded, and the new
    /// units' states.
    pub(super) fn begin_tick(
        mut budgets: ResMut<'_, ScriptBudgets>,
        mut failures: NonSendMut<'_, ScriptFailures>,
        mut new_states: ResMut<'_, NewUnitStates>,
    ) {
        budgets.begin_tick();
        failures.clear();
        new_states.clear();
    }

    /// Despawns each unit whose timed life ends with this tick, at its end, as the dead despawn: it
    /// is seen and sees in this tick's Vision stage for the last time.
    pub(super) fn end_lifespans(
        tick: Res<'_, SimTick>,
        units: Query<'_, '_, (Entity, &Lifespan)>,
        mut commands: Commands<'_, '_>,
    ) {
        let now = tick.start();
        for (entity, lifespan) in &units {
            if lifespan.ends_after(now) {
                commands.entity(entity).despawn();
            }
        }
    }
}
