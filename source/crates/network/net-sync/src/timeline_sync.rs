use crate::sync_adjustment::SyncAdjustment;
use crate::sync_config::SyncConfig;
use crate::sync_hysteresis::SyncHysteresis;
use crate::sync_sample::SyncSample;
use bevy_utils::prelude::DebugName;
use campfire_net_core::{TickDelta, TickInstant};
use tracing::trace;

/// Runtime controller for synchronizing a local clock or cursor to a remote timeline.
///
/// Implementors define their synchronization objective and how a large correction is applied.
/// The common controller sequence is provided by [`TimelineSync::sync`]: it waits for enough
/// latency samples, computes the current error, snaps on initial synchronization, and otherwise
/// slews the controlled clock through [`SyncHysteresis`].
///
/// This trait intentionally does not require
/// [`NetworkTimeline`](campfire_net_timeline::NetworkTimeline). `LocalTimelineSync` controls the
/// application-global `LocalTimeline` without owning a second clock, while interpolation owns and
/// directly adjusts its independent presentation timeline.
pub trait TimelineSync {
    /// Settings used by this synchronization policy.
    type Config;

    /// Return the mutable hysteresis state used by the common speed controller.
    fn sync_hysteresis(&mut self) -> &mut SyncHysteresis;

    /// Select the common controller settings from this policy's configuration.
    fn sync_config(config: &Self::Config) -> &SyncConfig;

    /// Get the ideal [`TickInstant`] that the controlled clock should follow.
    fn sync_objective(&self, sample: &SyncSample<'_, Self::Config>) -> TickInstant;

    /// Apply a discontinuous correction and return its whole-tick delta.
    ///
    /// `now` is explicit because not every controller owns the clock it disciplines.
    fn resync(&mut self, now: TickInstant, sync_objective: TickInstant) -> i32;

    /// Synchronize the controlled clock to the sample's remote timeline from its current `now`
    /// instant.
    ///
    /// Usually this slightly changes the relative speed. Initial synchronization and errors larger
    /// than [`SyncConfig::max_error_margin`] instead call [`TimelineSync::resync`]. The returned
    /// delta lets the caller propagate a discontinuity to any tick-indexed state it owns.
    fn sync(&mut self, now: TickInstant, sample: &SyncSample<'_, Self::Config>) -> Option<i32>
    where
        Self: Sized,
    {
        let config = sample.config;
        let sync_config = Self::sync_config(config);
        let objective = self.sync_objective(sample);
        let error_ticks = now - objective;
        let adjustment = if self.is_synced() {
            self.speed_adjustment(config, error_ticks)
        } else {
            SyncAdjustment::Resync
        };
        trace!(
            controller = ?DebugName::type_name::<Self>(),
            ?now,
            ?objective,
            ?adjustment,
            ?error_ticks,
            error_margin = ?sync_config.error_margin,
            max_error_margin = ?sync_config.max_error_margin,
            "timeline synchronization sample"
        );
        self.set_synced(true);

        match adjustment {
            SyncAdjustment::Resync => Some(self.resync(now, objective)),
            SyncAdjustment::SpeedAdjust(_) | SyncAdjustment::Nominal => None,
        }
    }

    /// Apply the common speed controller for an error measured in fractional ticks.
    ///
    /// A [`SyncAdjustment::Resync`] result is left to the synchronization policy.
    fn speed_adjustment(&mut self, config: &Self::Config, offset: TickDelta) -> SyncAdjustment {
        let adjustment = self
            .sync_hysteresis()
            .speed_adjustment(Self::sync_config(config), offset);
        match adjustment {
            SyncAdjustment::SpeedAdjust(ratio) => self.set_relative_speed(ratio),
            SyncAdjustment::Nominal => self.set_relative_speed(1.0),
            SyncAdjustment::Resync => {}
        }
        adjustment
    }

    fn is_synced(&self) -> bool;

    /// Set whether the timeline has completed its initial synchronization.
    ///
    /// The sync plugins set it for network clients; a session driver that needs no remote
    /// samples, such as an in-process host client, sets it directly.
    fn set_synced(&mut self, synced: bool);

    /// Returns the speed of your timeline relative to your system clock as an `f32`.
    /// A value of `1.0` means the timeline is running at normal speed.
    /// A value of `0.5` means the timeline is running at half speed,
    fn relative_speed(&self) -> f32;

    fn set_relative_speed(&mut self, ratio: f32);

    /// Reset the controller to its initial state (used when a client reconnects).
    fn reset(&mut self);
}
