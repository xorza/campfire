use bevy_ecs::entity::Entity;
use bevy_ecs::system::{Commands, NonSendMut, Query, Res, ResMut};
use campfire_script::{ScriptBudgets, ScriptFailures};
use campfire_sim::SimTick;

use crate::units::lifespan::Lifespan;
use crate::units::new_unit_states::NewUnitStates;

/// The core's systems that bound each tick: what a tick used cleared as it starts, the timed
/// lives that end with it, and the writes it left to units that never spawned.
#[derive(Debug)]
pub(crate) struct UnitTicks;

impl UnitTicks {
    /// Clears, as each tick starts, the script budgets' use and the failures recorded.
    pub(super) fn begin_tick(
        mut budgets: ResMut<'_, ScriptBudgets>,
        mut failures: NonSendMut<'_, ScriptFailures>,
    ) {
        budgets.begin_tick();
        failures.clear();
    }

    /// Drops, as each tick ends, the writes of its calls to units they made that never spawned:
    /// a unit takes its writes as it spawns within the tick.
    pub(super) fn drop_unspawned_states(mut new_states: ResMut<'_, NewUnitStates>) {
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
