use crate::clock_plugin::ClockPlugin;
use crate::local_timeline_plugin::LocalTimelinePlugin;
use bevy_app::{App, Plugin};
use bevy_time::TimePlugin;
use campfire_net_core::TickRate;

#[derive(Debug)]
pub struct TimelinePlugin {
    pub tick_rate: TickRate,
}

impl Plugin for TimelinePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<TimePlugin>() {
            app.add_plugins(TimePlugin);
        }
        if !app.is_plugin_added::<ClockPlugin>() {
            app.add_plugins(ClockPlugin);
        }
        app.add_plugins(LocalTimelinePlugin {
            tick_rate: self.tick_rate,
        });
    }
}
