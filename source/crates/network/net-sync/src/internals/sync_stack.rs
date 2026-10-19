use bevy_app::App;
use bevy_ecs::world::{EntityWorldMut, World};
use campfire_net_core::TickRate;
use campfire_net_stepper::Stack;
use campfire_net_transport::Transport;
use campfire_net_transport::internals::transport_stack::TransportStack;
use core::time::Duration;

use crate::client_sync_plugin::ClientSyncPlugin;
use crate::link_estimator::LinkEstimator;
use crate::link_estimator_config::LinkEstimatorConfig;
use crate::local_timeline_sync::LocalTimelineSync;
use crate::server_sync_plugin::ServerSyncPlugin;
use crate::timeline_sync::TimelineSync;

/// The transport layer with timeline sync on each link, ready once the client's input timeline is
/// synced.
#[derive(Debug, Clone, Copy, Default)]
pub struct SyncStack;

impl SyncStack {
    /// An estimate that takes every sample, and a transport that sends its tick every frame it
    /// sends nothing else, so that a test syncs in a few frames.
    fn sync_link(link: &mut EntityWorldMut<'_>) {
        link.insert(LinkEstimator::new(LinkEstimatorConfig {
            sample_interval: Duration::ZERO,
            ..LinkEstimatorConfig::default()
        }));
        link.get_mut::<Transport>()
            .expect("the transport stack puts a transport on each link")
            .set_heartbeat(Duration::ZERO);
    }
}

impl Stack for SyncStack {
    fn add_client(&self, app: &mut App, tick_rate: TickRate) {
        TransportStack.add_client(app, tick_rate);
        if !app.is_plugin_added::<ClientSyncPlugin>() {
            app.add_plugins(ClientSyncPlugin);
        }
    }

    fn add_server(&self, app: &mut App, tick_rate: TickRate) {
        TransportStack.add_server(app, tick_rate);
        if !app.is_plugin_added::<ServerSyncPlugin>() {
            app.add_plugins(ServerSyncPlugin);
        }
    }

    fn add_protocol(&self, app: &mut App) {
        TransportStack.add_protocol(app);
    }

    fn client_link(&self, link: &mut EntityWorldMut<'_>) {
        TransportStack.client_link(link);
        Self::sync_link(link);
    }

    fn server_link(&self, link: &mut EntityWorldMut<'_>) {
        TransportStack.server_link(link);
        Self::sync_link(link);
    }

    fn is_ready(&self, world: &World) -> bool {
        TransportStack.is_ready(world) && input_timeline_is_synced(world)
    }
}

/// Returns whether the application-global local timeline synchronization is ready.
pub fn input_timeline_is_synced(world: &World) -> bool {
    world
        .get_resource::<LocalTimelineSync>()
        .is_some_and(TimelineSync::is_synced)
}

#[cfg(test)]
mod tests {
    use super::*;
    use campfire_net_stepper::Stepper;
    use campfire_net_stepper::StepperConfig;

    /// `init` returns once the client's input timeline is synced.
    #[test]
    fn the_client_syncs() {
        let stepper = Stepper::new(SyncStack, StepperConfig::single());
        assert!(input_timeline_is_synced(stepper.client_app(0).world()));
    }
}
