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
