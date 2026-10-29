use bevy_app::{PluginGroup, PluginGroupBuilder};

use crate::client_connection_plugin::ClientConnectionPlugin;
use crate::network_topology_plugin::NetworkTopologyPlugin;

/// What both sides of a link connect with: the app's network topology and the lifecycle of each
/// client link, which a server's links to its clients take too.
#[derive(Debug)]
pub struct ConnectionPlugins;

impl PluginGroup for ConnectionPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(NetworkTopologyPlugin)
            .add(ClientConnectionPlugin)
    }
}
