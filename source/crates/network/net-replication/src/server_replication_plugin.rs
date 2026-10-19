use crate::control_plugin::ControlPlugin;
use crate::hierarchy_send_plugin::HierarchySendPlugin;
use crate::inheritance_plugin::InheritancePlugin;
use crate::network_visibility_plugin::NetworkVisibilityPlugin;
use crate::server_replicon_plugin::ServerRepliconPlugin;
use crate::server_send_plugin::ServerSendPlugin;
use bevy_app::prelude::{App, Plugin};
use bevy_replicon::server::ServerPlugin;

#[derive(Debug)]
pub struct ServerReplicationPlugin;

impl Plugin for ServerReplicationPlugin {
    fn build(&self, app: &mut App) {
        // We enable this Replicon setting when prediction/interpolation is active:
        // - replication mutations are sent every RepliconTick, even if there were 0 mutations.
        //   This avoids situations where a client mispredicted something, and the sender does
        //   not send any further corrections because nothing changed.
        // - it adds a `ServerMutateTicks` resource on the receiver that keeps track of the ticks
        //   where the receiver received any messages.
        app.add_plugins(ServerPlugin {
            tick_schedule: None,
            track_mutate_messages: true,
            ..Default::default()
        });
        app.add_plugins(ServerRepliconPlugin);
        app.add_plugins(ServerSendPlugin);
        app.add_plugins(ControlPlugin);
        app.add_plugins(HierarchySendPlugin);
        app.add_plugins(InheritancePlugin);
        app.add_plugins(NetworkVisibilityPlugin);
    }
}
