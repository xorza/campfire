//! The interpolation timeline, which trails the remote timeline by a delay that the sender's
//! replication interval sets: a sender announces it in its `SenderHello` when it connects.

use crate::interpolation_setup::InterpolationSetup;
use bevy_derive::{Deref, DerefMut};
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use campfire_net_core::TickInstant;
use campfire_net_core::TickSpan;
use campfire_net_sync::Deficit;
use campfire_net_sync::SyncHysteresis;
use campfire_net_sync::SyncSample;
use campfire_net_sync::SyncSetup;
use campfire_net_sync::TimelineSync;
use campfire_net_timeline::Timeline;
use core::time::Duration;
use tracing::trace;

/// The timeline the interpolation histories are sampled at: it trails the remote timeline at a
/// speed its sync controls, apart from the local one (11-net-stack/time.md, Two estimates of one
/// link keep two promises).
#[derive(Debug, Resource, Default, Deref, DerefMut, Reflect)]
pub struct InterpolationTimeline(Timeline<InterpolationSetup>);

impl TimelineSync for InterpolationTimeline {
    type Setup = InterpolationSetup;

    /// A step would show as a jump, and a presentation behind only shows older states.
    const DEFICIT: Deficit = Deficit::Slew;

    fn sync_hysteresis(&mut self) -> &mut SyncHysteresis {
        &mut self.state.hysteresis
    }

    fn sync_setup(setup: &Self::Setup) -> &SyncSetup {
        &setup.sync
    }

    fn sync_objective(&self, sample: &SyncSample<'_, InterpolationSetup>) -> TickInstant {
        let SyncSample {
            remote,
            setup,
            estimate,
            tick_rate,
        } = *sample;
        let delay =
            TickSpan::from_duration(setup.to_duration(self.remote_send_interval), tick_rate);
        // take extra margin if there is jitter
        let jitter_margin = TickSpan::from_duration(
            setup.sync.jitter_margin(estimate.jitter(), tick_rate),
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
        let tick_span = (target - now).to_i32();
        trace!(?tick_span, "Resync Interpolation timeline!");
        self.now = target;
        tick_span
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
    /// Resets the timeline for a new link: the controller, the presentation cursor and the remote
    /// send interval the previous link advertised.
    pub(crate) fn reset_link(&mut self) {
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

    use campfire_math::TickRate;

    use campfire_net_core::NetTick;
    use campfire_net_timeline::TimelineRate;

    use crate::interpolation_timeline_plugin::InterpolationTimelinePlugin;
    use crate::synced_interpolation_timeline::SyncedInterpolationTimeline;
    use bevy_app::Update;

    #[derive(Debug, Resource, Default)]
    struct SyncedParamRuns(u8);

    /// A controller reset keeps the cursor at tick 7; a link reset also clears the cursor
    /// and the send interval.
    #[test]
    fn a_reset_keeps_the_cursor_and_a_link_reset_clears_it() {
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

        timeline.reset_link();
        assert_eq!(timeline.now(), TickInstant::default());
        assert_eq!(timeline.remote_send_interval, Duration::ZERO);
    }

    #[test]
    fn the_interpolation_objective_trails_the_remote_by_its_margin() {
        let tick_rate = TickRate::try_from(100).unwrap();
        let mut remote = RemoteTimeline::default();
        remote.set_now(TickInstant::from(NetTick(100)));
        let setup = InterpolationSetup::default()
            .with_min_delay(Duration::from_millis(20))
            .with_send_interval_ratio(0.0);

        let objective = InterpolationTimeline::default().sync_objective(&SyncSample {
            remote: &remote,
            setup: &setup,
            estimate: LinkEstimate::new(Duration::ZERO, Duration::ZERO),
            tick_rate,
        });

        // remote 100 - min delay 2 ticks - jitter margin 1 tick.
        assert_eq!(objective, TickInstant::from(NetTick(97)));
    }

    #[test]
    fn the_synced_interpolation_timeline_waits_for_its_sync() {
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
    fn a_paused_virtual_time_holds_the_interpolation_timeline() {
        let mut app = App::new();
        let mut virtual_time = Time::<Virtual>::default();
        virtual_time.set_relative_speed(0.0);
        app.insert_resource(virtual_time);
        app.insert_resource(TimelineRate(TickRate::try_from(64).unwrap()));
        app.add_systems(PreUpdate, InterpolationTimelinePlugin::advance_timeline);

        app.init_resource::<InterpolationTimeline>();
        let before = app.world().resource::<InterpolationTimeline>().now();

        app.update();

        let after = app.world().resource::<InterpolationTimeline>().now();
        assert_eq!(after, before);
    }
}
