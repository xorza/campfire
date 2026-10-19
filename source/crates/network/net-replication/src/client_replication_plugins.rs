use crate::client_replication_plugin::ClientReplicationPlugin;
use crate::client_replicon_plugin::ClientRepliconPlugin;
use bevy_app::{PluginGroup, PluginGroupBuilder};
use bevy_replicon::client::ClientPlugin;

/// A client's replication, on [`ReplicationPlugins`](crate::ReplicationPlugins): Replicon's
/// client, the stack's receive, and what the client does with what it receives.
#[derive(Debug)]
pub struct ClientReplicationPlugins;

impl PluginGroup for ClientReplicationPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(ClientPlugin)
            .add(ClientRepliconPlugin)
            .add(ClientReplicationPlugin)
    }
}
