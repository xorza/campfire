use crate::host_plugin::HostPlugin;
use crate::raw_server_plugin::RawServerPlugin;
use crate::server_connection_plugin::ServerConnectionPlugin;
use bevy_app::{PluginGroup, PluginGroupBuilder};

/// A server's side of the connection, on [`ConnectionPlugins`](crate::ConnectionPlugins): its own
/// lifecycle, its host client and its raw links.
#[derive(Debug)]
pub struct ServerConnectionPlugins;

impl PluginGroup for ServerConnectionPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(ServerConnectionPlugin)
            .add(HostPlugin)
            .add(RawServerPlugin)
    }
}
