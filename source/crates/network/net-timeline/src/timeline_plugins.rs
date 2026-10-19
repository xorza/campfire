use crate::clock_plugin::ClockPlugin;
use crate::local_timeline_plugin::LocalTimelinePlugin;
use bevy_app::{PluginGroup, PluginGroupBuilder};
use campfire_math::TickRate;

/// The app's network clock and its local timeline at `tick_rate`, on Bevy's `TimePlugin`, which
/// the app adds.
#[derive(Debug)]
pub struct TimelinePlugins {
    pub tick_rate: TickRate,
}

impl PluginGroup for TimelinePlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(ClockPlugin)
            .add(LocalTimelinePlugin {
                tick_rate: self.tick_rate,
            })
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_app::{PluginGroup, PluginGroupBuilder};
    use bevy_time::TimePlugin;
    use campfire_math::TickRate;

    use crate::timeline_plugins::TimelinePlugins;

    /// [`TimelinePlugins`] on Bevy's `TimePlugin`, for a test app that has no other base.
    #[derive(Debug)]
    pub struct TestTimelinePlugins {
        pub tick_rate: TickRate,
    }

    impl PluginGroup for TestTimelinePlugins {
        fn build(self) -> PluginGroupBuilder {
            PluginGroupBuilder::start::<Self>()
                .add(TimePlugin)
                .add_group(TimelinePlugins {
                    tick_rate: self.tick_rate,
                })
        }
    }
}
