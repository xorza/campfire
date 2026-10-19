use bevy_app::{PluginGroup, PluginGroupBuilder};
use campfire_math::TickRate;

use crate::layers::{self, DEFAULT_TICK_RATE};

/// Every layer a server app runs. A protocol registers before or after it, until the app finishes
/// its plugins.
#[derive(Debug)]
pub struct ServerPlugins {
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
        layers::server(layers::shared(
            PluginGroupBuilder::start::<Self>(),
            self.tick_rate,
        ))
    }
}
