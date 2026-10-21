use super::*;
use bevy_app::{FixedMain, FixedUpdate};
use bevy_ecs::prelude::Resource;
use bevy_ecs::system::Res;
use bevy_time::{TimePlugin, TimeUpdateStrategy, Virtual};
use campfire_net_core::NetTick;

fn rate(hz: u32) -> TickRate {
    TickRate::try_from(hz).unwrap()
}

/// The event moves the rate from 64 Hz to 50 Hz, and the timestep with it, to 20 ms.
#[test]
fn set_tick_rate_updates_the_rate_and_the_timestep() {
    let mut app = App::new();
    app.add_plugins((
        TimePlugin,
        LocalTimelinePlugin {
            tick_rate: rate(64),
        },
    ));
    assert_eq!(
        app.world().resource::<Time<Fixed>>().timestep(),
        core::time::Duration::from_micros(15_625)
    );
    app.world_mut().trigger(SetTickRate(rate(50)));

    assert_eq!(app.world().resource::<TimelineRate>().0, rate(50));
    assert_eq!(
        app.world().resource::<Time<Fixed>>().timestep(),
        core::time::Duration::from_millis(20)
    );
}

/// Half of 21 Hz, 21 ticks every 2 s: a tick lasts 95,238,095 ns, so 2 s of 100 ms frames run
/// 21 ticks, the 21st at 1,999,999,995 ns, and the 22nd waits for 2,095,238,090 ns.
#[test]
fn half_of_21_hz_runs_21_ticks_in_2_seconds() {
    let half = TickRate::new(
        core::num::NonZeroU32::new(21).unwrap(),
        core::num::NonZeroU32::new(2).unwrap(),
    );
    let mut app = App::new();
    app.add_plugins((TimePlugin, LocalTimelinePlugin { tick_rate: half }));
    app.insert_resource(TimeUpdateStrategy::ManualDuration(
        core::time::Duration::from_millis(100),
    ));
    while app.world().resource::<Time<Virtual>>().elapsed() < core::time::Duration::from_secs(2) {
        app.update();
    }
    assert_eq!(
        app.world().resource::<Time<Virtual>>().elapsed(),
        core::time::Duration::from_secs(2)
    );
    assert_eq!(app.world().resource::<LocalTimeline>().tick(), NetTick(21));
}

#[derive(Resource, Default)]
struct Seen(Vec<Option<NetTick>>);

/// `FixedMain` simulates the tick it advanced to, 1 and then 2, in `FixedUpdate` and in
/// `FixedLast` alike; before and after it, no tick is simulated.
#[test]
fn the_simulated_tick_is_the_one_fixed_main_runs() {
    let mut app = App::new();
    app.add_plugins((
        TimePlugin,
        LocalTimelinePlugin {
            tick_rate: rate(64),
        },
    ));
    app.init_resource::<Seen>();
    let record = |timeline: Res<'_, LocalTimeline>, mut seen: ResMut<'_, Seen>| {
        seen.0.push(timeline.simulated_tick());
    };
    app.add_systems(FixedUpdate, record);
    app.add_systems(FixedLast, record);
    let simulated = |app: &App| app.world().resource::<LocalTimeline>().simulated_tick();

    assert_eq!(simulated(&app), None);
    app.world_mut().run_schedule(FixedMain);
    assert_eq!(simulated(&app), None);
    app.world_mut().run_schedule(FixedMain);
    assert_eq!(simulated(&app), None);

    assert_eq!(
        app.world().resource::<Seen>().0,
        [
            Some(NetTick(1)),
            Some(NetTick(1)),
            Some(NetTick(2)),
            Some(NetTick(2))
        ]
    );
}
