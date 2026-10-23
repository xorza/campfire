use super::*;

use bevy_app::App;
use bevy_app::PreUpdate;

use bevy_time::Real;
use bevy_time::Time;
use bevy_time::Virtual;
use campfire_net_sync::LinkEstimate;
use campfire_net_sync::RemoteTimeline;
use campfire_net_timeline::NetworkTimeline;

use campfire_math::TickRate;

use campfire_net_core::NetTick;
use campfire_net_timeline::TimelineRate;

use crate::interpolation_timeline_plugin::InterpolationTimelinePlugin;

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

/// The timeline advances by the frame's real time, clamped as the virtual time is, and not at
/// the virtual time's speed, which the local timeline's synchronization sets; a virtual time at
/// speed 0 or paused holds it. At 64 Hz a tick is 15.625 ms.
#[test]
fn the_interpolation_timeline_advances_by_the_clamped_real_time() {
    let tick = Duration::from_micros(15_625);
    let advance = |speed: f32, paused: bool, real_delta: Duration| {
        let mut app = App::new();
        let mut virtual_time = Time::<Virtual>::default();
        virtual_time.set_relative_speed(speed);
        if paused {
            virtual_time.pause();
        }
        let mut real = Time::<Real>::default();
        real.update_with_duration(Duration::ZERO);
        real.update_with_duration(real_delta);
        bevy_time::update_virtual_time(&mut Time::default(), &mut virtual_time, &real);
        app.insert_resource(virtual_time);
        app.insert_resource(real);
        app.insert_resource(TimelineRate(TickRate::try_from(64).unwrap()));
        app.add_systems(PreUpdate, InterpolationTimelinePlugin::advance_timeline);
        app.init_resource::<InterpolationTimeline>();
        app.update();
        app.world().resource::<InterpolationTimeline>().now()
    };
    let at = |ticks: u32| TickInstant::from(NetTick(ticks));

    // Two ticks of real time, whatever the virtual speed.
    assert_eq!(advance(1.0, false, tick * 2), at(2));
    assert_eq!(advance(2.0, false, tick * 2), at(2));
    // 500 ms of real time clamps to the virtual time's 250 ms: 250 / 15.625 = 16 ticks.
    assert_eq!(advance(1.0, false, Duration::from_millis(500)), at(16));
    assert_eq!(advance(0.0, false, tick * 2), at(0));
    assert_eq!(advance(1.0, true, tick * 2), at(0));
}
