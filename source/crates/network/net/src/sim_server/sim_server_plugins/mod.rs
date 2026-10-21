use std::num::NonZeroU32;

use bevy_app::{App, FixedUpdate, Plugin, PluginGroup, PluginGroupBuilder, Update};
use bevy_state::app::StatesPlugin;
use bevy_time::TimePlugin;
use campfire_app_threads::OneThreadPlugins;
use campfire_declarations::CapabilitySet;
use campfire_schedule::StrictSchedule;
use campfire_sim::MatchRate;

use crate::hosting::Hosting;
use crate::sim_server::SimServerPlugin;

/// The plugins of every server's app, as Bevy's `MinimalPlugins` groups its own: an app of one
/// thread, time and states, and `SimServerPlugin` at the session's rate `tick_hz`, whose
/// components of the mode's `capabilities` alone replicate, hosted as `hosting` says, its frame's
/// schedules strict.
/// Its app adds its run loop, where it runs one, its session and its transport.
#[derive(Debug)]
pub struct SimServerPlugins {
    pub tick_hz: NonZeroU32,
    pub capabilities: CapabilitySet,
    pub hosting: Hosting,
}

impl PluginGroup for SimServerPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add_group(OneThreadPlugins)
            .add(TimePlugin)
            .add(StatesPlugin)
            .add(SimServerPlugin {
                tick_rate: MatchRate::new(self.tick_hz).rate(),
                capabilities: self.capabilities,
                hosting: self.hosting,
            })
            .add(StrictFramePlugin)
    }
}

/// Makes the server's frame, `Update` and `FixedUpdate`, strict as the app finishes, after every
/// plugin added its systems, so the order of the frame's systems is the one its code states
/// (02-engine-core.md, The server's frame declares its order).
#[derive(Debug)]
struct StrictFramePlugin;

impl Plugin for StrictFramePlugin {
    fn build(&self, _: &mut App) {}

    fn cleanup(&self, app: &mut App) {
        app.edit_schedule(Update, StrictSchedule::apply);
        app.edit_schedule(FixedUpdate, StrictSchedule::apply);
    }
}

#[cfg(test)]
mod tests;
