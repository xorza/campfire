use core::time::Duration;

use bevy_reflect::Reflect;
use campfire_math::TickRate;
use campfire_net_core::PositiveTickSpan;

/// Immutable settings shared by local-clock and interpolation synchronization controllers.
///
/// The controllers keep their mutable hysteresis in `SyncHysteresis`. Local simulation uses these
/// settings to pace the application-global `LocalTimeline`; interpolation uses them to pace its
/// independent presentation timeline.
#[derive(Clone, Copy, Debug, Reflect)]
pub struct SyncSetup {
    /// How many jitters the delay margin takes: the margin a packet's delivery time takes is
    /// `RTT / 2 + jitter * jitter_multiple + jitter_margin`.
    ///
    /// The jitter is half the smoothed mean absolute deviation of the RTT, not its standard
    /// deviation, and the delay's distribution is not known, so no multiple promises a share of
    /// packets. For a normal RTT, the mean absolute deviation is `σ·√(2/π) ≈ 0.80 σ`, so one
    /// jitter is about `0.40 σ` of the RTT.
    pub jitter_multiple: u8,
    /// A tail margin on top of the jitter-derived one, in fractional ticks: room for the delays
    /// that the jitter term misses.
    ///
    /// The default is 1 tick, which interpolation keeps. The input timeline defaults it to 0,
    /// since its objective adds a tick for the input to reach the server before its tick.
    pub jitter_margin: PositiveTickSpan,
    /// The error, in ticks, inside which the controlled timeline runs at the nominal speed: on
    /// both sides of its objective, or ahead of it alone for a timeline that steps a deficit.
    pub error_margin: PositiveTickSpan,
    /// The error, in ticks, past which the controlled timeline snaps to its objective.
    pub max_error_margin: PositiveTickSpan,
    /// How many consecutive errors are allowed before we start adjusting the speed
    pub consecutive_errors_threshold: u8,
    /// By how much should we speed up the simulation to make ticks stay in sync with server?
    pub speedup_factor: f32,
}

impl SyncSetup {
    /// Total jitter-based safety margin as a `Duration`, combining the measured
    /// jitter scaled by [`Self::jitter_multiple`] with the fixed fractional-tick
    /// margin in [`Self::jitter_margin`].
    pub fn jitter_margin(&self, jitter: Duration, tick_rate: TickRate) -> Duration {
        jitter * u32::from(self.jitter_multiple) + self.jitter_margin.to_duration(tick_rate)
    }
}

impl Default for SyncSetup {
    fn default() -> Self {
        Self {
            jitter_multiple: 4,
            jitter_margin: PositiveTickSpan::from_ticks(1),
            error_margin: PositiveTickSpan::from_ticks(1),
            max_error_margin: PositiveTickSpan::from_ticks(10),
            consecutive_errors_threshold: 3,
            speedup_factor: 1.05,
        }
    }
}
