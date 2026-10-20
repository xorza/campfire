use crate::server_connection_plugin::ServerConnectionPlugin;
use bevy_app::{PluginGroup, PluginGroupBuilder};

/// A server's side of the connection, on [`ConnectionPlugins`](crate::ConnectionPlugins): its own
/// lifecycle and its links.
#[derive(Debug)]
pub struct ServerConnectionPlugins;

impl PluginGroup for ServerConnectionPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>().add(ServerConnectionPlugin)
    }
}
