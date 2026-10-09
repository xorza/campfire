//! A player's saves and loads on a local server: a save makes a checkpoint at the boundary after
//! the next tick, unless the mode alone saves; a quick load goes back to the latest save, its
//! server starts again on its data from it, at the speed the match plays at, every client plays
//! on from it, and the published log's segments verify. A save due as the server stopped begins
//! as its restore reaches the boundary.

use std::fs;
use std::num::NonZeroU32;

use bevy_time::{Time, Virtual};
use campfire_capabilities::{SaveBy, SavesData};
use campfire_common::{PlayerSlot, Tick};
use campfire_net::internals::{End, InProcessMatch, LinkModel, MatchSetup};
use campfire_net::{
    InputsDiscarded, JoinState, PaceSpeed, PendingSaves, SaveCommand, SaveRefused, ServerDir,
    SessionDir, SimServer,
};
use campfire_protocol::{JournalFrames, Outcome, SeedChain, SessionLog};
use campfire_runner::{InputRules, Runner, Session};
use campfire_sim::TickRate;
use lightyear::core::tick::TickDuration;

use crate::scratch::Scratch;

/// A chain of room for three checkpoints.
const SEED_CHAIN: SeedChain = SeedChain::new([9; 32], NonZeroU32::new(4).unwrap());

/// The scenario's match, its mode's `[saves] by` `save_by`, its server keeping its data in
/// `data`, after 30 steps.
fn playing(data: &Scratch, save_by: SaveBy) -> InProcessMatch {
    let saves = SavesData {
        by: save_by,
        autosave_ms: None,
    };
    let mut local = started(data, saves);
    for _ in 0..30 {
        local.step();
    }
    local
}

/// The scenario's match, its mode's `[saves]` `saves`, its server keeping its data in `data`, as
/// it starts.
fn started(data: &Scratch, saves: SavesData) -> InProcessMatch {
    let setup = MatchSetup {
        saves,
        ..MatchSetup::duo(LinkModel::PERFECT, SEED_CHAIN)
    };
    let mut local = InProcessMatch::new(setup);
    local.keep_data(data.path().to_owned());
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

fn playing_on(local: &InProcessMatch) -> bool {
    (0..2).all(|client| {
        let state = local.client(client).world().resource::<JoinState>();
        state.clock().is_some()
    })
}

#[test]
fn a_players_save_is_refused_when_the_mode_alone_saves() {
    let data = Scratch::new();
    let mut local = playing(&data, SaveBy::Mode);
    command(&mut local, SaveCommand::Save);
    for _ in 0..10 {
        local.step();
    }
    assert_eq!(log(&local).segment(), 0);
    assert_eq!(
        local.log().take::<SaveRefused>(),
        [SaveRefused {
            reason: "the mode alone saves".to_owned(),
        }]
    );
}

#[test]
fn a_quick_load_goes_back_to_the_save_and_every_client_plays_on_from_it() {
    let data = Scratch::new();
    let mut local = playing(&data, SaveBy::Player);

    // The save: a checkpoint at the boundary after the tick that runs next, segment 1.
    command(&mut local, SaveCommand::Save);
    for _ in 0..10 {
        local.step();
    }
    SimServer::settle_checkpoint(local.server_mut().world_mut());
    let save = log(&local).checkpoints().next().unwrap().clone();
    assert_eq!(save.segment, 1);
    for _ in 0..30 {
        local.step();
    }
    assert!(log(&local).next_tick() > save.tick);

    // The quick load, at speed 2: the server starts again on its data, at that speed, the match
    // at the save's tick and in its state, the log holding nothing after it.
    local.pace().set_speed(PaceSpeed::Double);
    command(&mut local, SaveCommand::LoadLatest);
    for _ in 0..10 {
        local.step();
        if log(&local).next_tick() == save.tick {
            break;
        }
    }
    let world = local.server().world();
    assert_eq!(log(&local).next_tick(), save.tick);
    assert_eq!(
        world.resource::<Session>().state_hash(world),
        save.state_hash
    );
    // At speed 2 a tick lasts half the session's, and a frame runs the LAN's max input delay of
    // 10 ticks less one at most.
    let tick = TickRate::new(local.packages().manifest().tick_hz.default()).length();
    assert_eq!(InputRules::LAN.max_input_delay.get(), 10);
    assert_eq!(world.resource::<TickDuration>().0, tick / 2);
    assert_eq!(world.resource::<Time<Virtual>>().max_delta(), tick / 2 * 9);
    assert_eq!(log(&local).checkpoints().collect::<Vec<_>>(), [&save]);

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
    discarded.sort_by_key(|event| event.slot);
    let slots = [0, 1].map(PlayerSlot::new);
    assert_eq!(
        discarded,
        slots.map(|slot| InputsDiscarded { slot, count: 1 })
    );

    // The published log holds the segment before the save and the one after it, and verifies.
    SimServer::end_session(local.server_mut().world_mut(), Outcome::Aborted).unwrap();
    let file =
        SessionDir::publish(local.server().world().resource::<ServerDir>(), log(&local)).unwrap();
    let published = SessionLog::decode(&fs::read(file).unwrap()).unwrap();
    assert_eq!(published.checkpoints().collect::<Vec<_>>(), [&save]);
    let ticks = published.next_tick();
    let seeds = published.revealed_seeds().unwrap();
    let mut replay = Runner::new(published.rewound(), seeds, local.packages()).unwrap();
    while replay.log().next_tick() < ticks {
        if replay.log().next_tick() == save.tick {
            assert_eq!(replay.state_hash(), save.state_hash);
        }
        replay.run_tick();
    }
    assert_eq!(replay.check_result(), Ok(()));
}

#[test]
fn a_save_due_as_the_server_stopped_begins_as_its_restore_reaches_the_boundary() {
    // An autosave every second, every 30 ticks of the lane mode.
    let data = Scratch::new();
    let saves = SavesData {
        by: SaveBy::Player,
        autosave_ms: NonZeroU32::new(1000),
    };
    let mut local = started(&data, saves);
    for _ in 0..40 {
        local.step();
    }
    SimServer::settle_checkpoint(local.server_mut().world_mut());
    let save = log(&local).checkpoints().next().unwrap().clone();
    assert_eq!((save.segment, save.tick), (1, Tick::new(30)));
    let id = log(&local).session_id();
    local.stop_server();

    // A stop after tick 29, before the save began: the journal ends before its begin, and its
    // snapshot is gone.
    let session = data.path().join("sessions").join(id.to_string());
    let journal = session.join("journal");
    let bytes = fs::read(&journal).unwrap();
    let mut frames = JournalFrames::new(&bytes).unwrap();
    let begun = loop {
        let start = frames.whole();
        let record = frames.next().expect("the journal holds the save's begin");
        if record[0] == 6 {
            break start;
        }
    };
    fs::write(&journal, &bytes[..begun]).unwrap();
    let snapshot = session.join("snapshots").join(save.snapshot.to_string());
    fs::remove_file(&snapshot).unwrap();

    // The restore replays to tick 30 and begins the save there: the same record, its snapshot
    // written again.
    local.restart_server();
    assert_eq!(local.next_tick(End::Server), 30);
    SimServer::settle_checkpoint(local.server_mut().world_mut());
    assert_eq!(log(&local).checkpoint_at(save.tick), Some(&save));
    assert!(snapshot.exists());
}
