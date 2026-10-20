//! The app's network clock and its timelines in the ECS: the [`Clock`] each frame reads, the
//! [`LocalTimeline`] of the fixed ticks, the timelines a network layer keeps, and the
//! [`TimelinePlugins`] that add them (11-net-stack/layering.md, The vocabulary is plain, and the
//! clock and the timelines a glue crate of their own).

#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

mod clock;
mod clock_plugin;
mod clock_source;
mod local_timeline;
mod local_timeline_plugin;
mod local_timeline_shift;
mod network_timeline;
mod rollback;
mod set_tick_rate;
mod timeline;
mod timeline_plugins;
mod timeline_rate;
mod timeline_setup;
mod timeline_systems;

pub use crate::clock::Clock;
pub use crate::clock_plugin::ClockPlugin;
pub use crate::clock_source::ClockSource;
pub use crate::local_timeline::LocalTimeline;
pub use crate::local_timeline_shift::LocalTimelineShift;
pub use crate::network_timeline::NetworkTimeline;
pub use crate::rollback::Rollback;
pub use crate::set_tick_rate::SetTickRate;
pub use crate::timeline::Timeline;
pub use crate::timeline_plugins::TimelinePlugins;
pub use crate::timeline_rate::TimelineRate;
pub use crate::timeline_setup::TimelineSetup;
pub use crate::timeline_systems::TimelineSystems;

/// What the tests of the layers above share: the timeline on Bevy's time.
#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::timeline_plugins::internals::TestTimelinePlugins;
}
