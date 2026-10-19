use bevy_app::{PluginGroup, PluginGroupBuilder};
use campfire_net_connection::HostPlugin;
use campfire_net_connection::RawServerPlugin;
use campfire_net_core::TickRate;
use campfire_net_link::ServerLinkPlugin;
use campfire_net_replication::ServerReplicationPlugin;
use campfire_net_sync::ServerSyncPlugin;

use crate::shared_plugin::{DEFAULT_TICK_RATE, SharedPlugin};

/// A plugin group containing all the server plugins. A protocol registers before or after it,
/// until the app finishes its plugins.
#[derive(Debug)]
pub struct ServerPlugins {
    /// The tick interval for the server. This is used to determine how often the server should
    /// tick. The default value is 1/60 seconds.
    pub tick_rate: TickRate,
}

impl Default for ServerPlugins {
    fn default() -> Self {
        Self {
            tick_rate: DEFAULT_TICK_RATE,
        }
    }
}

impl PluginGroup for ServerPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(ServerSyncPlugin)
            .add(ServerLinkPlugin)
            .add(SharedPlugin {
                tick_rate: self.tick_rate,
            })
            .add(ServerReplicationPlugin)
            .add(HostPlugin)
            .add(RawServerPlugin)
    }
}
