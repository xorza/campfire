use bevy_app::{App, First, Plugin};
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_time::{TimePlugin, TimeSystems};

use crate::clock::Clock;

/// Inserts the app's [`Clock`] on its default source unless the app holds one already, and
/// refreshes it at the start of each frame, after Bevy's time update.
#[derive(Debug)]
pub struct ClockPlugin;

impl Plugin for ClockPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<TimePlugin>() {
            app.add_plugins(TimePlugin);
        }
        app.init_resource::<Clock>();
        app.add_systems(First, Clock::update.after(TimeSystems));
    }
}
