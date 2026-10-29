use bevy_ecs::query::Without;
use bevy_ecs::system::{Commands, Local, Query, Res, ResMut};
use campfire_sim::{DueIndex, DueUnit, SimTick};

use crate::dead::Dead;
use crate::lifespan::Lifespan;
use crate::new_unit_states::NewUnitStates;

/// The core's systems that bound each tick: the timed lives that end with it, and the writes it
/// left to units that never spawned.
#[derive(Debug)]
pub(crate) struct UnitTicks;

impl UnitTicks {
    /// Drops, as each tick ends, the writes of its calls to units they made that never spawned:
    /// a unit takes its writes as it spawns within the tick.
    pub(super) fn drop_unspawned_states(mut new_states: ResMut<'_, NewUnitStates>) {
        new_states.clear();
    }

    /// Despawns each unit whose timed life ends with this tick, at its end, as the dead despawn: it
    /// is seen and sees in this tick's Vision stage for the last time. It visits only the units
    /// the index of the lives that end holds due, in a buffer it keeps. A dead unit is its
    /// death's to despawn, which may keep it for a later stage, so its life waits until it lives
    /// again.
    pub(super) fn end_lifespans(
        tick: Res<'_, SimTick>,
        mut index: ResMut<'_, DueIndex<Lifespan>>,
        units: Query<'_, '_, &Lifespan, Without<Dead>>,
        mut commands: Commands<'_, '_>,
        mut due: Local<'_, Vec<DueUnit>>,
    ) {
        let now = tick.start();
        due.clear();
        index.take_due(now, &mut due);
        for &DueUnit { entity, .. } in &*due {
            if units
                .get(entity)
                .is_ok_and(|lifespan| lifespan.ends_after(now))
            {
                commands.entity(entity).despawn();
            }
        }
    }
}
