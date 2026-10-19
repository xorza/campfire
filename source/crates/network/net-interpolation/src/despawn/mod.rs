use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use campfire_net_despawn::DespawnReplicated;
use campfire_net_replication::DelayedDespawn;
use campfire_net_replication::InterpolatedDespawn;
use campfire_net_timeline::NetworkTimeline;

use crate::interpolation_systems::InterpolationSystems;
use crate::interpolation_timeline::InterpolationTimeline;

/// Delays the despawn of an interpolated entity until the interpolation timeline shows it.
pub(crate) fn delay_interpolated_despawns(app: &mut App) {
    app.insert_resource(InterpolatedDespawn::AtInterpolationTick);
    app.add_systems(
        Update,
        despawn_interpolated_entities.in_set(InterpolationSystems::Prepare),
    );
}

pub(crate) fn despawn_interpolated_entities(
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

#[cfg(test)]
mod tests;
