//! Syncs a client's clocks to its server.
use bevy_app::prelude::*;
use bevy_ecs::prelude::IntoScheduleConfigs;
use campfire_net_connection::Client;
use campfire_net_timeline::TimelineSystems;

use crate::local_timeline_sync::LocalTimelineSync;
use crate::remote_timeline::RemoteTimeline;

/// Keeps a client's estimate of its server's timeline, and syncs the local timeline to it.
#[derive(Debug)]
pub(crate) struct ClientSyncPlugin;

impl Plugin for ClientSyncPlugin {
    fn build(&self, app: &mut App) {
        app.register_required_components::<Client, RemoteTimeline>();
        // `ClientSyncPlugins` builds the local timeline's sync, which this observer reconfigures,
        // first.
        app.add_observer(LocalTimelineSync::recompute_input_delay_on_setup_update);

        app.add_observer(RemoteTimeline::handle_connect);
        app.add_observer(RemoteTimeline::update_from_packet);
        app.add_systems(
            PreUpdate,
            RemoteTimeline::advance.in_set(TimelineSystems::Advance),
        );
        app.add_systems(Last, RemoteTimeline::reset_received_packet);
    }
}

#[cfg(test)]
mod tests;
