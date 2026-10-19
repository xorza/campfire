use bevy_app::App;
use bevy_ecs::world::{EntityWorldMut, World};
use campfire_net_core::TickRate;
use campfire_net_replication::internals::replication_stack::ReplicationStack;
use campfire_net_stepper::Stack;
use campfire_net_sync::TimelineSync;
use campfire_net_sync::internals::sync_stack::SyncStack;

use crate::internals::protocol::InterpolationProtocolPlugin;
use crate::interpolation_marker_plugin::InterpolationMarkerPlugin;
use crate::interpolation_plugin::InterpolationPlugin;
use crate::interpolation_registry_plugin::InterpolationRegistryPlugin;
use crate::interpolation_timeline::InterpolationTimeline;

/// Replication and sync, with interpolation's registry on both sides and its systems on the
/// client, ready once both of its timelines are synced.
#[derive(Debug, Clone, Copy, Default)]
pub struct InterpolationStack;

impl InterpolationStack {
    /// The registry and markers on both sides, as `SharedPlugin` adds them.
    fn add_shared(app: &mut App) {
        if !app.is_plugin_added::<InterpolationMarkerPlugin>() {
            app.add_plugins((InterpolationMarkerPlugin, InterpolationRegistryPlugin));
        }
    }
}

impl Stack for InterpolationStack {
    fn add_client(&self, app: &mut App, tick_rate: TickRate) {
        SyncStack.add_client(app, tick_rate);
        ReplicationStack.add_client(app, tick_rate);
        Self::add_shared(app);
        if !app.is_plugin_added::<InterpolationPlugin>() {
            app.add_plugins(InterpolationPlugin);
        }
    }

    fn add_server(&self, app: &mut App, tick_rate: TickRate) {
        SyncStack.add_server(app, tick_rate);
        ReplicationStack.add_server(app, tick_rate);
        Self::add_shared(app);
    }

    fn add_protocol(&self, app: &mut App) {
        ReplicationStack.add_protocol(app);
        SyncStack.add_protocol(app);
        if !app.is_plugin_added::<InterpolationProtocolPlugin>() {
            app.add_plugins(InterpolationProtocolPlugin);
        }
    }

    fn client_link(&self, link: &mut EntityWorldMut<'_>) {
        SyncStack.client_link(link);
        ReplicationStack.client_link(link);
    }

    fn server_link(&self, link: &mut EntityWorldMut<'_>) {
        SyncStack.server_link(link);
        ReplicationStack.server_link(link);
    }

    fn is_ready(&self, world: &World) -> bool {
        SyncStack.is_ready(world) && interpolation_timeline_is_synced(world)
    }
}

/// Returns whether the application-global presentation timeline is ready.
pub fn interpolation_timeline_is_synced(world: &World) -> bool {
    world
        .get_resource::<InterpolationTimeline>()
        .is_some_and(TimelineSync::is_synced)
}

#[cfg(test)]
mod tests {
    use super::*;
    use campfire_net_stepper::Stepper;
    use campfire_net_stepper::StepperConfig;

    /// `init` returns once both of the client's timelines are synced, the host's included.
    #[test]
    fn both_timelines_sync() {
        let stepper = Stepper::new(InterpolationStack, StepperConfig::host_server());
        assert!(interpolation_timeline_is_synced(
            stepper.client_app(0).world()
        ));
    }
}
