use crate::interpolation_systems::InterpolationSystems;
use crate::interpolation_timeline::InterpolationTimeline;
use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use campfire_net_despawn::DespawnReplicated;
use campfire_net_replication::{DelayedDespawn, InterpolatedDespawn};
use campfire_net_timeline::NetworkTimeline;

/// A client's interpolation between two consecutive replicated updates of each component.
#[derive(Debug, Default)]
pub(crate) struct InterpolationPlugin;

impl Plugin for InterpolationPlugin {
    fn build(&self, app: &mut App) {
        // An interpolated entity's despawn waits until the interpolation timeline shows it.
        app.insert_resource(InterpolatedDespawn::AtInterpolationTick);
        app.add_systems(
            Update,
            Self::despawn_interpolated_entities.in_set(InterpolationSystems::Prepare),
        );

        // The timeline these sets read is synced in the previous `PostUpdate` and advanced in
        // `PreUpdate`, and the histories are received there too: the schedules order them.
        app.configure_sets(
            Update,
            (
                InterpolationSystems::Prepare,
                InterpolationSystems::Interpolate,
            )
                .in_set(InterpolationSystems::All)
                .chain(),
        );
    }
}

impl InterpolationPlugin {
    /// Despawns each entity whose delayed despawn's tick the interpolation timeline reached.
    fn despawn_interpolated_entities(
        interpolation: Res<'_, InterpolationTimeline>,
        query: Query<'_, '_, (Entity, &DelayedDespawn)>,
        mut commands: Commands<'_, '_>,
    ) {
        let interpolation_tick = interpolation.now().tick();
        for (entity, despawn) in &query {
            if interpolation_tick >= despawn.tick() {
                commands.entity(entity).queue_silenced(DespawnReplicated);
            }
        }
    }
}

#[cfg(test)]
mod tests;
