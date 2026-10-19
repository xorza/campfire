//! Clock sync between peers: the latency estimate from the session's RTT samples, the remote
//! timeline, and the controller that keeps a local timeline on it.

#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]
#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]

mod client_sync_plugin;
mod client_sync_plugins;
#[cfg(feature = "internals")]
mod fixtures;
mod input_delay_config;
mod input_timeline_config;
mod link_estimate;
mod link_estimator;
mod link_estimator_config;
mod link_estimator_plugin;
mod local_timeline_sync;
mod local_timeline_sync_plugin;
mod remote_timeline;
mod rtt_estimator_ewma;
mod sync_adjustment;
mod sync_config;
mod sync_hysteresis;
mod sync_plugins;
mod sync_sample;
mod sync_systems;
mod synced_local_timeline;
mod timeline_sync;
mod timeline_sync_plugin;

pub use crate::client_sync_plugins::ClientSyncPlugins;
pub use crate::input_delay_config::InputDelayConfig;
pub use crate::input_timeline_config::InputTimelineConfig;
pub use crate::link_estimate::LinkEstimate;
pub use crate::link_estimator::LinkEstimator;
pub use crate::link_estimator_config::LinkEstimatorConfig;
pub use crate::local_timeline_sync::LocalTimelineSync;
pub use crate::remote_timeline::RemoteEstimate;
pub use crate::remote_timeline::RemoteTimeline;
pub use crate::remote_timeline::RemoteTimelineConfig;
pub use crate::sync_adjustment::SyncAdjustment;
pub use crate::sync_config::SyncConfig;
pub use crate::sync_hysteresis::SyncHysteresis;
pub use crate::sync_plugins::SyncPlugins;
pub use crate::sync_sample::SyncSample;
pub use crate::sync_systems::SyncSystems;
pub use crate::synced_local_timeline::SyncedLocalTimeline;
pub use crate::timeline_sync::TimelineSync;

/// What the tests and benches of the layers above share: this layer's stack, test protocol,
/// fixtures and measurements.
#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::fixtures::sync_stack::{SyncStack, input_timeline_is_synced};
}
