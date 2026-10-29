use bevy_app::{App, Plugin, Update};
use bevy_ecs::archetype::Archetypes;
use bevy_ecs::component::Components;
use bevy_ecs::prelude::*;
use campfire_net_despawns::DespawnReplicated;
use campfire_net_replication::{DelayedDespawn, InterpolatedDespawn};
use campfire_net_timeline::NetworkTimeline;

use crate::interpolation_holders::InterpolationHolders;
use crate::interpolation_phase_systems::InterpolationPhaseSystems;
use crate::interpolation_registry::InterpolationRegistry;
use crate::interpolation_systems::InterpolationSystems;
use crate::interpolation_timeline::InterpolationTimeline;

/// A client's interpolation between two consecutive replicated updates of each component: the
/// stages of [`InterpolationSystems::Prepare`], where each rule and maintained component installs
/// its typed systems once the registry builds it, and the holders that resolve the archetypes
/// before each stage.
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

        app.init_resource::<InterpolationHolders>();
        app.configure_sets(
            Update,
            (
                InterpolationPhaseSystems::RefreshHistory,
                InterpolationPhaseSystems::History,
                InterpolationPhaseSystems::RefreshApply,
                InterpolationPhaseSystems::Apply,
            )
                .chain()
                .in_set(InterpolationSystems::Prepare),
        );
        app.add_systems(
            Update,
            (
                Self::refresh_delayed_holders.in_set(InterpolationPhaseSystems::RefreshHistory),
                Self::refresh_delayed_holders.in_set(InterpolationPhaseSystems::RefreshApply),
            ),
        );
    }
}

impl InterpolationPlugin {
    /// Resolves the delayed interpolation of the archetypes created since the last refresh.
    fn refresh_delayed_holders(
        mut holders: ResMut<'_, InterpolationHolders>,
        archetypes: &Archetypes,
        components: &Components,
        registry: Res<'_, InterpolationRegistry>,
    ) {
        registry.refresh_holders(&mut holders, archetypes, components);
    }

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
