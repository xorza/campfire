//! The interpolation timeline, which trails the remote timeline by a delay that the sender's
//! replication interval sets: a sender announces it in its `SenderHello` when it connects.

use crate::interpolation_config::InterpolationConfig;
use bevy_derive::{Deref, DerefMut};
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use campfire_net_core::TickDelta;
use campfire_net_core::TickInstant;
use campfire_net_sync::SyncConfig;
use campfire_net_sync::SyncHysteresis;
use campfire_net_sync::SyncSample;
use campfire_net_sync::TimelineSync;
use campfire_net_timeline::Timeline;
use core::time::Duration;
use tracing::trace;

/// Application-global presentation timeline used to sample interpolation histories.
///
/// Unlike the local simulation clock, this is a genuine independent timeline: it intentionally
/// trails remote simulation time and advances at its own synchronization-controlled speed.
#[derive(Debug, Resource, Default, Deref, DerefMut, Reflect)]
pub struct InterpolationTimeline(Timeline<InterpolationConfig>);

impl TimelineSync for InterpolationTimeline {
    type Config = InterpolationConfig;

    fn sync_hysteresis(&mut self) -> &mut SyncHysteresis {
        &mut self.state.hysteresis
    }

    fn sync_config(config: &Self::Config) -> &SyncConfig {
        &config.sync
    }

    fn sync_objective(&self, sample: &SyncSample<'_, InterpolationConfig>) -> TickInstant {
        let SyncSample {
            remote,
            config,
            estimate,
            tick_rate,
        } = *sample;
        let delay =
            TickDelta::from_duration(config.to_duration(self.remote_send_interval), tick_rate);
        // take extra margin if there is jitter
        let jitter_margin = TickDelta::from_duration(
            config.sync.jitter_margin(estimate.jitter(), tick_rate),
            tick_rate,
        );
        let target = remote.current_estimate();
        let obj = target - (delay + jitter_margin);
        trace!(
            ?target,
            ?delay,
            ?jitter_margin,
            send_interval = ?self.remote_send_interval,
            "InterpolationTimeline objective: {:?}", obj
        );
        obj
    }

    fn resync(&mut self, now: TickInstant, sync_objective: TickInstant) -> i32 {
        let target = sync_objective;
        let tick_delta = (target - now).to_i32();
        trace!(?tick_delta, "Resync Interpolation timeline!");
        self.now = target;
        tick_delta
    }

    fn is_synced(&self) -> bool {
        self.is_synced
    }

    fn set_synced(&mut self, synced: bool) {
        self.is_synced = synced;
    }

    fn relative_speed(&self) -> f32 {
        self.relative_speed
    }

    fn set_relative_speed(&mut self, ratio: f32) {
        self.relative_speed = ratio;
    }

    /// Resets the synchronization controller, and keeps the presentation cursor.
    fn reset(&mut self) {
        self.hysteresis = SyncHysteresis::default();
        self.is_synced = false;
        self.relative_speed = 1.0;
    }
}

impl InterpolationTimeline {
    /// Resets the timeline for a new session: the controller, the presentation cursor and the
    /// remote send interval the previous session advertised.
    pub(crate) fn reset_session(&mut self) {
        self.reset();
        self.now = TickInstant::default();
        self.remote_send_interval = Duration::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use bevy_app::App;
    use bevy_app::PreUpdate;

    use bevy_time::Time;
    use bevy_time::Virtual;
    use campfire_net_sync::LinkEstimate;
    use campfire_net_sync::RemoteTimeline;
    use campfire_net_timeline::NetworkTimeline;

    use campfire_net_core::NetTick;
    use campfire_net_core::TickRate;

    use crate::interpolation_timeline_plugin::InterpolationTimelinePlugin;
    use crate::synced_interpolation_timeline::SyncedInterpolationTimeline;
    use bevy_app::Update;

    #[derive(Debug, Resource, Default)]
    struct SyncedParamRuns(u8);

    /// A controller reset keeps the cursor at tick 7; a session reset also clears the cursor and
    /// the send interval.
    #[test]
    fn reset_keeps_the_cursor_and_a_session_reset_clears_it() {
        let mut timeline = InterpolationTimeline::default();
        timeline.set_now(TickInstant::from(NetTick(7)));
        timeline.set_synced(true);
        timeline.set_relative_speed(1.5);
        timeline.remote_send_interval = Duration::from_millis(50);

        timeline.reset();
        assert!(!timeline.is_synced());
        assert_eq!(timeline.relative_speed(), 1.0);
        assert_eq!(timeline.now(), TickInstant::from(NetTick(7)));
        assert_eq!(timeline.remote_send_interval, Duration::from_millis(50));

        timeline.reset_session();
        assert_eq!(timeline.now(), TickInstant::default());
        assert_eq!(timeline.remote_send_interval, Duration::ZERO);
    }

    #[test]
    fn interpolation_timeline_objective_lags_remote_by_delay_margin() {
        let tick_rate = TickRate::try_from(100).unwrap();
        let mut remote = RemoteTimeline::default();
        remote.set_now(TickInstant::from(NetTick(100)));
        let config = InterpolationConfig::default()
            .with_min_delay(Duration::from_millis(20))
            .with_send_interval_ratio(0.0);

        let objective = InterpolationTimeline::default().sync_objective(&SyncSample {
            remote: &remote,
            config: &config,
            estimate: LinkEstimate::new(Duration::ZERO, Duration::ZERO),
            tick_rate,
        });

        // remote 100 - min delay 2 ticks - jitter margin 1 tick.
        assert_eq!(objective, TickInstant::from(NetTick(97)));
    }

    #[test]
    fn synced_interpolation_timeline_checks_presentation_readiness() {
        fn require_synced_timeline(
            timeline: SyncedInterpolationTimeline<'_, '_>,
            mut runs: ResMut<'_, SyncedParamRuns>,
        ) {
            let _ = timeline.now();
            runs.0 += 1;
        }

        let mut app = App::new();
        app.init_resource::<InterpolationTimeline>();
        app.init_resource::<SyncedParamRuns>();
        app.add_systems(Update, require_synced_timeline);

        app.update();
        assert_eq!(app.world().resource::<SyncedParamRuns>().0, 0);

        app.world_mut()
            .resource_mut::<InterpolationTimeline>()
            .set_synced(true);

        app.update();
        assert_eq!(app.world().resource::<SyncedParamRuns>().0, 1);
    }

    #[test]
    fn paused_virtual_time_does_not_advance_interpolation_timeline() {
        let mut app = App::new();
        let mut virtual_time = Time::<Virtual>::default();
        virtual_time.set_relative_speed(0.0);
        app.insert_resource(virtual_time);
        app.insert_resource(TickRate::try_from(64).unwrap());
        app.add_systems(PreUpdate, InterpolationTimelinePlugin::advance_timeline);

        app.init_resource::<InterpolationTimeline>();
        let before = app.world().resource::<InterpolationTimeline>().now();

        app.update();

        let after = app.world().resource::<InterpolationTimeline>().now();
        assert_eq!(after, before);
    }
}
