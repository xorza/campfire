use core::time::Duration;

use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use campfire_net_sync::SyncSetup;
use campfire_net_timeline::TimelineSetup;

use crate::interpolation_state::InterpolationState;

/// Setup to specify how delayed interpolation should behave
#[derive(Resource, Clone, Copy, Debug, Reflect)]
pub struct InterpolationSetup {
    /// The least delay interpolation applies: big enough that an interpolated entity always has
    /// a server snapshot to interpolate towards. [`Duration::ZERO`] leaves the delay to the ratio.
    pub min_delay: Duration,
    /// Base interpolation delay as a ratio of the server's replication interval.
    ///
    /// The effective delay also includes [`SyncSetup::jitter_multiple`] times the measured jitter
    /// and [`SyncSetup::jitter_margin`] simulation ticks.
    /// 0 leaves the delay to `min_delay`.
    pub send_interval_ratio: f32,
    pub sync: SyncSetup,
}

impl Default for InterpolationSetup {
    fn default() -> Self {
        Self {
            min_delay: Duration::from_millis(5),
            send_interval_ratio: 1.2,
            sync: SyncSetup::default(),
        }
    }
}

impl InterpolationSetup {
    #[must_use]
    pub const fn with_min_delay(mut self, min_delay: Duration) -> Self {
        self.min_delay = min_delay;
        self
    }

    #[must_use]
    pub const fn with_send_interval_ratio(mut self, send_interval_ratio: f32) -> Self {
        self.send_interval_ratio = send_interval_ratio;
        self
    }

    /// How much behind the latest server update we want the interpolation time to be
    pub(crate) fn to_duration(self, server_send_interval: Duration) -> Duration {
        let ratio_value = server_send_interval.mul_f32(self.send_interval_ratio);
        core::cmp::max(ratio_value, self.min_delay)
    }
}

impl TimelineSetup for InterpolationSetup {
    type State = InterpolationState;
}
