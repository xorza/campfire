//! A player's saves and loads on a local server: a save makes a checkpoint at the boundary after
//! the next tick, unless the mode alone saves; a quick load goes back to the latest save, its
//! server starts again on its data from it, at the speed the match plays at, every client plays
//! on from it, and the published log's segments verify. A save due as the server stopped begins
//! as its restore reaches the boundary.

use std::iter;
use std::num::NonZeroU32;

use bevy_time::{Time, Virtual};
use campfire_common::{Seat, Tick};
use campfire_harness::{End, InProcessMatch, InProcessSetup, LinkModel};
use campfire_mode::{SaveBy, SavesData};
use campfire_net::internals::SimServerWorldInternals;
use campfire_net::{
    InputsDiscarded, JoinState, PaceSpeed, PendingSaves, SaveCommand, SaveRefusal, SaveRefused,
    ServerDir, SessionDir, SimServerWorld,
};
use campfire_protocol::{InputRules, JournalFrames, SeedChain, SessionLog};
use campfire_runner::{Replay, Session, SessionWorld};
use campfire_sim::{MatchRate, SimWorld};
use campfire_store::InputFile;
use campfire_store::internals::TestDir;
use lightyear::core::tick::TickDuration;

/// A chain of room for three checkpoints.
const SEED_CHAIN: SeedChain = SeedChain::new([9; 32], NonZeroU32::new(4).unwrap());

/// The scenario's match, its mode's `[saves] by` `save_by`, its server keeping its data in
/// `scratch`, after 30 steps.
fn playing(test_dir: &TestDir, save_by: SaveBy) -> InProcessMatch {
    let saves = SavesData {
        by: save_by,
        autosave_ms: None,
    };
    let mut local = started(test_dir, saves);
    for _ in 0..30 {
        local.step();
    }
    local
}

/// The scenario's match, its mode's `[saves]` `saves`, its server keeping its data in `scratch`, as
/// it starts.
fn started(test_dir: &TestDir, saves: SavesData) -> InProcessMatch {
    let setup = InProcessSetup {
        saves,
        ..InProcessSetup::duo(LinkModel::PERFECT, SEED_CHAIN)
    };
    let mut local = InProcessMatch::new(setup);
    local.keep_data(test_dir.path("data"));
    local.start_match();
    local.play_by_team(InProcessMatch::SCENARIO_SCRIPTS);
    local
}

fn command(local: &mut InProcessMatch, command: SaveCommand) {
    let world = local.client_mut(0).world_mut();
    world.resource_mut::<PendingSaves>().push(command);
}

fn log(local: &InProcessMatch) -> &SessionLog {
    local.server().world().resource::<Session>().log()
}

/// How many loads the journal of `local`'s session, kept in `scratch`, records.
fn loads(test_dir: &TestDir, local: &InProcessMatch) -> usize {
    let id = log(local).session_id().to_string();
    let journal = test_dir
        .path("data")
        .join("sessions")
        .join(id)
        .join("journal");
    let bytes = test_dir.read(&journal);
    let mut frames = JournalFrames::new(&bytes).unwrap();
    // A load's record is the journal's kind 7.
    iter::from_fn(|| frames.next())
        .filter(|record| record[0] == 7)
        .count()
}

fn playing_on(local: &InProcessMatch) -> bool {
    (0..2).all(|client| {
        let state = local.client(client).world().resource::<JoinState>();
        state.playing_clock().is_some()
    })
}

#[test]
fn a_players_save_is_refused_when_the_mode_alone_saves() {
    let test_dir = TestDir::new();
    let mut local = playing(&test_dir, SaveBy::Mode);
    command(&mut local, SaveCommand::Save);
    for _ in 0..10 {
        local.step();
    }
    assert_eq!(log(&local).segment(), 0);
    assert_eq!(
        local.log().take::<SaveRefused>(),
        [SaveRefused {
            refusal: SaveRefusal::ByMode,
        }]
    );
}

#[test]
fn a_quick_load_goes_back_to_the_save_and_every_client_plays_on_from_it() {
    let test_dir = TestDir::new();
    let mut local = playing(&test_dir, SaveBy::Player);

    // The save: a checkpoint at the boundary after the tick that runs next, segment 1.
    command(&mut local, SaveCommand::Save);
    for _ in 0..10 {
        local.step();
    }
    local.server_mut().world_mut().settle_checkpoint();
    let save = log(&local).checkpoints().next().unwrap().clone();
    assert_eq!(save.segment, 1);
    for _ in 0..30 {
        local.step();
    }
    assert!(log(&local).next_tick() > save.tick);

    // The quick load, at speed 2, pressed twice in one frame: the server loads once, and starts
    // again on its data, at that speed, the match at the save's tick and in its state, the log
    // holding nothing after it.
    local.pace().set_speed(PaceSpeed::Double);
    command(&mut local, SaveCommand::LoadLatest);
    command(&mut local, SaveCommand::LoadLatest);
    for _ in 0..10 {
        local.step();
        if log(&local).next_tick() == save.tick {
            break;
        }
    }
    let world = local.server().world();
    assert_eq!(log(&local).next_tick(), save.tick);
    assert_eq!(world.state_hash(), save.state_hash);
    // At speed 2 a tick lasts half the session's, and a frame runs the LAN's max input delay of
    // 10 ticks less one at most.
    let tick = MatchRate::new(local.packages().manifest().tick_hz.default()).length();
    assert_eq!(InputRules::LAN.max_delay.get(), 10);
    assert_eq!(world.resource::<TickDuration>().0, tick / 2);
    assert_eq!(world.resource::<Time<Virtual>>().max_delta(), tick / 2 * 9);
    assert_eq!(log(&local).checkpoints().collect::<Vec<_>>(), [&save]);
    assert_eq!(loads(&test_dir, &local), 1);

    // Every client comes back and plays on from the save.
    for _ in 0..300 {
        local.step();
        if playing_on(&local) && local.next_tick(End::Server) > save.tick.get() + 30 {
            break;
        }
    }
    assert!(playing_on(&local));
    assert!(local.next_tick(End::Server) > save.tick.get() + 30);
    // Each player's order after the save never applies.
    let mut discarded = local.log().take::<InputsDiscarded>();
    discarded.sort_by_key(|event| event.seat);
    let seats = [0, 1].map(Seat::new);
    assert_eq!(
        discarded,
        seats.map(|seat| InputsDiscarded { seat, count: 1 })
    );

    // The published log holds the segment before the save and the one after it, and verifies.
    local.server_mut().world_mut().end_session().unwrap();
    local.server_mut().world_mut().session_mut().close_journal();
    let held = log(&local);
    let revealed = held.revealed_seeds().unwrap();
    let last = revealed.seed(held.segment()).unwrap().clone();
    let id = held.session_id();
    let data = local.server().world().resource::<ServerDir>();
    let file = SessionDir::publish(data, id, &last).unwrap();
    let bytes = InputFile::read(&file, SessionLog::MAX_FILE_LEN).unwrap();
    let published = SessionLog::decode(&bytes).unwrap();
    assert_eq!(published.checkpoints().collect::<Vec<_>>(), [&save]);
    let mut replay = Replay::of(published, local.packages()).unwrap();
    assert_eq!(replay.run(), Ok(()));
}

#[test]
fn a_save_due_as_the_server_stopped_begins_as_its_restore_reaches_the_boundary() {
    // An autosave every second, every 30 ticks of the lane mode.
    let test_dir = TestDir::new();
    let saves = SavesData {
        by: SaveBy::Player,
        autosave_ms: NonZeroU32::new(1000),
    };
    let mut local = started(&test_dir, saves);
    for _ in 0..40 {
        local.step();
    }
    local.server_mut().world_mut().settle_checkpoint();
    let save = log(&local).checkpoints().next().unwrap().clone();
    assert_eq!((save.segment, save.tick), (1, Tick::new(30)));
    let id = log(&local).session_id();
    local.stop_server();

    // A stop after tick 29, before the save began: the journal ends before its begin, and its
    // snapshot is gone.
    let session = test_dir.path("data").join("sessions").join(id.to_string());
    let journal = session.join("journal");
    let bytes = test_dir.read(&journal);
    let mut frames = JournalFrames::new(&bytes).unwrap();
    let begun = loop {
        let start = frames.whole();
        let record = frames.next().expect("the journal holds the save's begin");
        if record[0] == 6 {
            break start;
        }
    };
    test_dir.write(&journal, &bytes[..begun]);
    let snapshot = session.join("snapshots").join(save.snapshot.to_string());
    test_dir.remove(&snapshot);

    // The restore replays to tick 30 and begins the save there: the same record, its snapshot
    // written again.
    local.restart_server();
    assert_eq!(local.next_tick(End::Server), 30);
    local.server_mut().world_mut().settle_checkpoint();
    assert_eq!(log(&local).checkpoint_at(save.tick), Some(&save));
    assert!(test_dir.exists(&snapshot));
}
