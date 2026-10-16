use std::num::NonZeroU32;

use bevy_app::{PluginGroup, PluginGroupBuilder, TaskPoolPlugin};
use bevy_state::app::StatesPlugin;
use bevy_time::TimePlugin;
use campfire_declare::CapabilitySet;
use campfire_sim::TickRate;

use crate::hosting::Hosting;
use crate::sim_server::SimServer;

/// The plugins of every server's app, as Bevy's `MinimalPlugins` groups its own: the task pool,
/// time and states, and `SimServer` at the session's rate `tick_hz`, whose components of the
/// mode's `capabilities` alone replicate, hosted as `hosting` says. Its app adds its run loop, where it runs one, its
/// session and its transport.
#[derive(Debug)]
pub struct SimServerPlugins {
    pub tick_hz: NonZeroU32,
    pub capabilities: CapabilitySet,
    pub hosting: Hosting,
}

impl PluginGroup for SimServerPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(TaskPoolPlugin::default())
            .add(TimePlugin)
            .add(StatesPlugin)
            .add(SimServer {
                tick: TickRate::new(self.tick_hz).length(),
                capabilities: self.capabilities,
                hosting: self.hosting,
            })
    }
}
