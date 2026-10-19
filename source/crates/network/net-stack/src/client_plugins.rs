use bevy_app::{PluginGroup, PluginGroupBuilder};
use campfire_math::TickRate;

use crate::layers::{self, DEFAULT_TICK_RATE};

/// Every layer a client app runs. A protocol registers before or after it, until the app finishes
/// its plugins.
#[derive(Debug)]
pub struct ClientPlugins {
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
        layers::client(layers::shared(
            PluginGroupBuilder::start::<Self>(),
            self.tick_rate,
        ))
    }
}
