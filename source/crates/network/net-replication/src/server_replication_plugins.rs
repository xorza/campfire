use crate::control_plugin::ControlPlugin;
use crate::hierarchy_send_plugin::HierarchySendPlugin;
use crate::inheritance_plugin::InheritancePlugin;
use crate::network_visibility_plugin::NetworkVisibilityPlugin;
use crate::server_replicon_plugin::ServerRepliconPlugin;
use crate::server_send_plugin::ServerSendPlugin;
use bevy_app::{PluginGroup, PluginGroupBuilder};
use bevy_replicon::server::ServerPlugin;

/// A server's replication, on [`ReplicationPlugins`](crate::ReplicationPlugins): Replicon's
/// server, the stack's send, control, hierarchy, inheritance and visibility.
#[derive(Debug)]
pub struct ServerReplicationPlugins;

impl PluginGroup for ServerReplicationPlugins {
    fn build(self) -> PluginGroupBuilder {
        // Replicon sends mutations every tick, even with none, so a client that mispredicted gets
        // its correction although nothing changed; and its receiver keeps `ServerMutateTicks`,
        // the ticks it received messages at.
        PluginGroupBuilder::start::<Self>()
            .add(ServerPlugin {
                tick_schedule: None,
                track_mutate_messages: true,
                ..Default::default()
            })
            .add(ServerRepliconPlugin)
            .add(ServerSendPlugin)
            .add(ControlPlugin)
            .add(HierarchySendPlugin)
            .add(InheritancePlugin)
            .add(NetworkVisibilityPlugin)
    }
}
