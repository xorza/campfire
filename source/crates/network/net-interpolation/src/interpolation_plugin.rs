use crate::delayed_interpolation_plugin::DelayedInterpolationPlugin;
use crate::despawn::delay_interpolated_despawns;
use crate::interpolation_registry_plugin::InterpolationRegistryPlugin;
use crate::interpolation_systems::InterpolationSystems;
use crate::interpolation_timeline_plugin::InterpolationTimelinePlugin;
use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;

/// Plugin that enables interpolating between replicated updates received from the remote, on the
/// client.
///
/// Each remote update will be stored in a buffer, and the component will smoothly interpolate
/// between two consecutive remote updates.
#[derive(Debug, Default)]
pub struct InterpolationPlugin;

impl Plugin for InterpolationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(InterpolationTimelinePlugin);

        if !app.is_plugin_added::<InterpolationRegistryPlugin>() {
            app.add_plugins(InterpolationRegistryPlugin);
        }
        delay_interpolated_despawns(app);

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
        app.add_plugins(DelayedInterpolationPlugin);
    }
}
