use bevy_app::{PluginGroup, PluginGroupBuilder};
use campfire_net_connection::RawClientPlugin;
use campfire_net_core::TickRate;
use campfire_net_interpolation::InterpolationPlugin;
use campfire_net_prediction::PredictionPlugin;
use campfire_net_replication::ClientReplicationPlugin;
use campfire_net_sync::ClientSyncPlugin;

use crate::shared_plugin::{DEFAULT_TICK_RATE, SharedPlugin};

/// A plugin group containing all the client plugins. A protocol registers before or after it,
/// until the app finishes its plugins.
#[derive(Debug)]
pub struct ClientPlugins {
    /// The tick interval for the client. This is used to determine how often the client should
    /// tick. The default value is 1/60 seconds.
    pub tick_rate: TickRate,
}

impl Default for ClientPlugins {
    fn default() -> Self {
        Self {
            tick_rate: DEFAULT_TICK_RATE,
        }
    }
}

impl PluginGroup for ClientPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(ClientSyncPlugin)
            .add(SharedPlugin {
                tick_rate: self.tick_rate,
            })
            .add(ClientReplicationPlugin)
            .add(PredictionPlugin)
            .add(InterpolationPlugin)
            .add(RawClientPlugin)
    }
}
