//! A local match's console, through its client's: reads of the running match, each naming the
//! tick it read; a run to a tick, until a unit's probe state, past its wall time or cancelled,
//! each pausing the match at the tick it reached, its reply after the client received that
//! tick; reads during a run, answered before it; and reads that change no state, so the
//! checkpoint's copy of the state still equals it and the published log replays to the hash the
//! console gave.

use std::num::NonZeroU32;

use bevy_app::FixedPostUpdate;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_bots::OrderScript;
use campfire_capabilities::ProbeState;
use campfire_common::Tick;
use campfire_console::{
    ConsoleAnswer, ConsoleAsk, Detail, Reply, Request, RequestId, RunAsk, RunCondition, RunStop,
    RunTo, SessionPhase, UnitActivity, UnitSelection, UnitsAsk,
};
use campfire_harness::MatchUnits;
use campfire_host::ReceivedTick;
use campfire_host::internals::TickHashes;
use campfire_net::ClientConsole;
use campfire_net::internals::SimServerWorldInternals;
use campfire_net_harness::{End, InProcessMatch, InProcessSetup};
use campfire_net_prediction::RollbackMode;
use campfire_protocol::SeedChain;
use campfire_runner::Session;
use campfire_sim::{SimTick, SimWorld};
use campfire_store::internals::TestDir;

use crate::{published_replay, snapshots};

/// The most frames a reply takes.
const PATIENCE: usize = 400;

/// The chain of the session, of room for a checkpoint.
const SEED_CHAIN: SeedChain = SeedChain::new([9; 32], NonZeroU32::new(2).unwrap());

/// After each tick the server ran, whether the walker of player slot 0 walked and stood idle.
#[derive(Resource, Debug, Default)]
struct WalkerStates(Vec<(u64, bool, bool)>);

/// Records the walker's states after the tick the server ran, once a tick.
fn record_walker(world: &mut World) {
    let ran = world.resource::<SimTick>().start().get() - 1;
    let walker = MatchUnits::of(world).hero(0);
    let walks = ProbeState::Walking.holds(world, walker);
    let idle = ProbeState::Idle.holds(world, walker);
    let mut states = world.resource_mut::<WalkerStates>();
    if states.0.last().is_none_or(|&(tick, ..)| tick < ran) {
        states.0.push((ran, walks, idle));
    }
}

/// Queues `ask`, of the id `id`, on client 0's console.
fn ask(local: &mut InProcessMatch, id: u64, ask: ConsoleAsk) {
    let mut console = local
        .client_mut(0)
        .world_mut()
        .resource_mut::<ClientConsole>();
    console.push([Request {
        id: RequestId(id),
        ask,
    }]);
}

/// Steps until client 0's console gives the reply to `id`; every reply it gave, in its order,
/// that one among them.
fn until_reply(local: &mut InProcessMatch, id: u64) -> Vec<Reply> {
    let mut replies = Vec::new();
    for _ in 0..PATIENCE {
        local.step();
        let world = local.client_mut(0).world_mut();
        world
            .resource_mut::<ClientConsole>()
            .take_replies(&mut replies);
        if replies.iter().any(|reply| reply.id == RequestId(id)) {
            return replies;
        }
    }
    panic!("no reply to request {id} in {PATIENCE} frames: {replies:?}");
}

/// The one reply to `id`, which comes alone.
fn reply(local: &mut InProcessMatch, id: u64) -> Reply {
    let mut replies = until_reply(local, id);
    assert_eq!(replies.len(), 1, "{replies:?}");
    replies.pop().unwrap()
}

const fn run(to: RunTo, wall_seconds: Option<u32>) -> ConsoleAsk {
    ConsoleAsk::Run(RunAsk { to, wall_seconds })
}

/// The last tick the server ran.
fn server_ran(local: &InProcessMatch) -> u64 {
    local.next_tick(End::Server) - 1
}

/// Asserts that the run of `reply` ended for `stop` at the tick the server stands paused after,
/// and that the client received that tick before the reply came.
fn paused_at(local: &InProcessMatch, reply: &Reply, stop: RunStop) -> u64 {
    assert_eq!(reply.answer, ConsoleAnswer::Ran(stop), "{reply:?}");
    let tick = reply.tick.unwrap();
    assert!(local.pace().is_paused());
    assert_eq!(server_ran(local), tick);
    let received = local.client(0).world().resource::<ReceivedTick>().get();
    assert!(received.unwrap().get() > tick, "{received:?}");
    tick
}

/// The state hash the status of `reply` gives.
fn status_hash(reply: Reply) -> String {
    let ConsoleAnswer::Status(status) = reply.answer else {
        panic!("{reply:?}");
    };
    assert_eq!(
        (status.phase, status.rate),
        (SessionPhase::Running, Some(30))
    );
    status.state_hash.unwrap()
}

/// The hash of the server's state after `tick`, as the server recorded it.
fn hash_after(local: &InProcessMatch, tick: u64) -> String {
    let hashes = local.server().world().resource::<TickHashes>();
    hashes.get()[usize::try_from(tick).unwrap()].to_string()
}

#[test]
fn a_console_runs_the_local_match_and_reads_it_between_two_ticks() {
    let test_dir = TestDir::new();
    let mut local = InProcessMatch::new(InProcessSetup::solo(RollbackMode::Check, 1, SEED_CHAIN));
    local.keep_data(test_dir.path("data"));
    local.start_match();
    let script = "[[order]]\ntick = 30\ndirective.orders.move.to = [4, -2]\n";
    local.play(0, &OrderScript::parse(script).unwrap());
    local.open_console(0);
    let server = local.server_mut();
    server.init_resource::<WalkerStates>();
    server.add_systems(FixedPostUpdate, record_walker);
    let walker = MatchUnits::of(local.server().world()).hero(0);

    // A status names the tick it read and the state's hash after it.
    ask(&mut local, 1, ConsoleAsk::Status);
    let status = reply(&mut local, 1);
    let read = status.tick.unwrap();
    assert_eq!(status_hash(status), hash_after(&local, read));

    // The walker stands until its order of tick 30, then walks to its point and stands again:
    // each run ends after the first tick its probe shows the state.
    let until = |state| {
        run(
            RunTo::Until(RunCondition::Unit {
                id: walker.get(),
                state,
            }),
            None,
        )
    };
    ask(&mut local, 2, until(UnitActivity::Walking));
    let ended = reply(&mut local, 2);
    let walks = paused_at(&local, &ended, RunStop::Held);
    ask(&mut local, 3, until(UnitActivity::Idle));
    let ended = reply(&mut local, 3);
    let stands = paused_at(&local, &ended, RunStop::Held);
    let states = local.server().world().resource::<WalkerStates>().0.clone();
    let first = |from: u64, holds: fn(&(u64, bool, bool)) -> bool| {
        let after = states.iter().filter(|&&(tick, ..)| tick > from);
        after.copied().find(holds).map(|(tick, ..)| tick)
    };
    assert_eq!(first(read, |&(_, walks, _)| walks), Some(walks));
    assert_eq!(first(walks, |&(_, _, idle)| idle), Some(stands));
    assert!(walks >= 30 && stands > walks, "{walks}, {stands}");

    // A run to tick 300 answers a status asked during it first, then pauses after tick 300.
    ask(&mut local, 4, run(RunTo::Tick(300), None));
    for _ in 0..3 {
        local.step();
    }
    ask(&mut local, 5, ConsoleAsk::Status);
    let during = reply(&mut local, 5);
    assert!(during.tick.unwrap() < 300, "{during:?}");
    let ended = reply(&mut local, 4);
    paused_at(&local, &ended, RunStop::Reached);
    ask(&mut local, 6, ConsoleAsk::Status);
    let at_300 = reply(&mut local, 6);
    assert_eq!(at_300.tick, Some(300));
    let hash_300 = status_hash(at_300);
    assert_eq!(hash_300, hash_after(&local, 300));

    // A run past its wall time, none here, and a cancelled one pause at the tick they reached.
    ask(&mut local, 7, run(RunTo::Tick(1_000_000), Some(0)));
    let ended = reply(&mut local, 7);
    paused_at(&local, &ended, RunStop::WallTime);
    ask(&mut local, 8, run(RunTo::Tick(1_000_000), None));
    for _ in 0..3 {
        local.step();
    }
    ask(&mut local, 9, ConsoleAsk::Cancel(RequestId(8)));
    let mut cancelled = until_reply(&mut local, 8);
    assert_eq!(cancelled.len(), 2, "{cancelled:?}");
    let ran = cancelled.pop().unwrap();
    assert_eq!(cancelled[0].answer, ConsoleAnswer::Cancelled);
    paused_at(&local, &ran, RunStop::Cancelled);

    // Reads every frame of a run that takes a checkpoint: its copy of the state, which change
    // ticks keep, still equals the state its replay restores.
    let next = local.next_tick(End::Server);
    local
        .server_mut()
        .world_mut()
        .request_checkpoint(Tick::new(next + 5));
    ask(&mut local, 10, run(RunTo::Ticks(10), None));
    let units = ConsoleAsk::Units(UnitsAsk {
        units: UnitSelection::All,
        detail: Detail::Detailed,
        first: 0,
        limit: None,
    });
    let mut id = 100;
    loop {
        ask(&mut local, id, ConsoleAsk::Status);
        ask(&mut local, id + 1, units.clone());
        id += 2;
        let replies = until_reply(&mut local, id - 1);
        if replies.iter().any(|reply| reply.id == RequestId(10)) {
            break;
        }
    }
    local.server_mut().world_mut().settle_checkpoint();
    let checkpoints = (local.server().world().resource::<Session>().log())
        .checkpoints()
        .count();
    assert_eq!(checkpoints, 1);
    let snapshots = snapshots(&test_dir, &local);
    let mut replay = published_replay(&mut local, &snapshots);
    replay.run_to(Tick::new(301), |_| {}).unwrap();
    assert_eq!(replay.game().state_hash().to_string(), hash_300);
}
