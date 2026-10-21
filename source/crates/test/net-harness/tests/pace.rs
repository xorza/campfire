//! A local match's pause and speed: paused, no end runs a sim tick, and the match resumes at the
//! tick it stopped before; at speed 2, each tick lasts half the session's tick, so a frame of the
//! clock as long as a session's tick runs two, but for the nanosecond the half rounds up.

use std::time::Duration;

use bevy_time::{Fixed, Time};
use campfire_net::{JoinState, PaceSpeed};
use campfire_net_emulator::LinkModel;
use campfire_net_harness::{End, InProcessMatch, InProcessSetup};
use campfire_sim::MatchRate;

#[test]
fn a_paused_match_runs_no_tick_and_at_speed_2_runs_two_ticks_a_frame() {
    let mut local = InProcessMatch::new(InProcessSetup::duo(
        LinkModel::perfect(),
        InProcessMatch::SEED_CHAIN,
    ));
    local.start_match();
    local.play_by_team(InProcessMatch::SCENARIO_SCRIPTS);
    for _ in 0..30 {
        local.step();
    }
    let ticks = |local: &InProcessMatch| {
        [End::Server, End::Client(0), End::Client(1)].map(|end| local.next_tick(end))
    };

    // Paused for 60 frames: no end runs a tick.
    local.pace().set_paused(true);
    let paused = ticks(&local);
    for _ in 0..60 {
        local.step();
    }
    assert_eq!(ticks(&local), paused);

    // Again at normal speed: the server runs the tick it stopped before, then one a frame.
    local.pace().set_paused(false);
    let [server, ..] = paused;
    for frames in 1..=5 {
        local.step();
        assert_eq!(local.next_tick(End::Server), server + frames);
    }

    // At speed 2, a tick lasts half the session's, rounded up to the nanosecond: 16,666,667 ns of
    // 33,333,333. A frame, a session's tick long, runs as many as the time the fixed clock held
    // over and the frame's make, and holds the rest over: two, or one when less than a
    // nanosecond was held, as two need a nanosecond more than a frame.
    let tick = MatchRate::new(local.packages().manifest().tick_hz.default()).length();
    let half = Duration::from_nanos(16_666_667);
    assert_eq!(tick, Duration::from_nanos(33_333_333));
    let mut held = local.server().world().resource::<Time<Fixed>>().overstep();
    local.pace().set_speed(PaceSpeed::Double);
    let mut next = local.next_tick(End::Server);
    for _ in 0..30 {
        local.step();
        let ran = (held + tick).as_nanos() / half.as_nanos();
        held = held + tick - half * u32::try_from(ran).unwrap();
        next += u64::try_from(ran).unwrap();
        assert_eq!(local.next_tick(End::Server), next);
    }
    let fixed = local.server().world().resource::<Time<Fixed>>();
    assert_eq!((fixed.timestep(), fixed.overstep()), (half, held));
    for client in 0..2 {
        let state = local.client(client).world().resource::<JoinState>();
        assert!(state.playing_clock().is_some(), "client {client} plays on");
    }
}
