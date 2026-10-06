//! A local match's pause and speed: paused, no end runs a sim tick, and the match resumes at the
//! tick it stopped before; at speed 2, each tick lasts half the session's tick, so a frame of the
//! clock as long as a session's tick runs two.

use bevy_time::{Fixed, Time};
use campfire_net::internals::{End, LinkModel, LocalMatch, MatchSetup};
use campfire_net::{JoinState, Speed};
use campfire_sim::TickRate;

#[test]
fn a_paused_match_runs_no_tick_and_at_speed_2_runs_two_ticks_a_frame() {
    let mut local = LocalMatch::new(MatchSetup::duo(LinkModel::PERFECT, LocalMatch::SEED_CHAIN));
    local.start_match();
    local.play_by_team(LocalMatch::SCENARIO_SCRIPTS);
    for _ in 0..30 {
        local.step();
    }
    let ticks = |local: &LocalMatch| {
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

    // At speed 2, a tick lasts half the session's: a frame, a session's tick long, runs two, the
    // first with the time the fixed clock held over too, as many more as half ticks it holds.
    let tick = TickRate::new(local.packages().manifest().tick_hz.default()).length();
    let held = local.server().world().resource::<Time<Fixed>>().overstep();
    local.pace().set_speed(Speed::Double);
    let start = local.next_tick(End::Server);
    local.step();
    let first = 2 + u64::try_from(held.as_nanos() / (tick / 2).as_nanos()).unwrap();
    assert_eq!(local.next_tick(End::Server), start + first);
    for frames in 1..=30 {
        local.step();
        assert_eq!(local.next_tick(End::Server), start + first + 2 * frames);
    }
    for client in 0..2 {
        let state = local.client(client).world().resource::<JoinState>();
        assert!(state.clock().is_some(), "client {client} plays on");
    }
}
