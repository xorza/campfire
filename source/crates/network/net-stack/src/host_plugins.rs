use bevy_app::{PluginGroup, PluginGroupBuilder};
use campfire_net_core::TickRate;

use crate::layers::{self, DEFAULT_TICK_RATE};

/// Every layer a server app that holds a host client runs: the shared layers once, then the
/// client's and the server's own. A protocol registers before or after it, until the app finishes
/// its plugins.
#[derive(Debug)]
pub struct HostPlugins {
    pub tick_rate: TickRate,
}

impl Default for HostPlugins {
    fn default() -> Self {
        Self {
            tick_rate: DEFAULT_TICK_RATE,
        }
    }
}

impl PluginGroup for HostPlugins {
    fn build(self) -> PluginGroupBuilder {
        let shared = layers::shared(PluginGroupBuilder::start::<Self>(), self.tick_rate);
        layers::server(layers::client(shared))
    }
}
