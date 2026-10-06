//! A player's saves and loads on a local server: a save makes a checkpoint at the boundary after
//! the next tick, unless the mode alone saves; a quick load goes back to the latest save, its
//! server starts again on its data from it, every client plays on from it, and the published
//! log's segments verify.

use std::fs;
use std::num::NonZeroU32;

use campfire_capabilities::SaveBy;
use campfire_common::PlayerSlot;
use campfire_net::internals::{End, LinkModel, LocalMatch, MatchSetup};
use campfire_net::{
    InputsDiscarded, JoinState, PendingSaves, SaveCommand, SaveRefused, SessionDir, SimServer,
};
use campfire_protocol::{Outcome, SeedChain, SessionLog};
use campfire_runner::{Runner, Session};

use crate::Scratch;

/// A chain of room for three checkpoints.
const SEED_CHAIN: SeedChain = SeedChain::new([9; 32], NonZeroU32::new(4).unwrap());

/// The scenario's match, its mode's `[saves] by` `save_by`, its server keeping its data in
/// `data`, after 30 steps.
fn playing(data: &Scratch, save_by: SaveBy) -> LocalMatch {
    let setup = MatchSetup {
        save_by,
        ..MatchSetup::duo(LinkModel::PERFECT, SEED_CHAIN)
    };
    let mut local = LocalMatch::new(setup);
    local.keep_data(data.0.clone());
    local.start_match();
    local.play_by_team(LocalMatch::SCENARIO_SCRIPTS);
    for _ in 0..30 {
        local.step();
    }
    local
}

fn command(local: &mut LocalMatch, command: SaveCommand) {
    let world = local.client_mut(0).world_mut();
    world.resource_mut::<PendingSaves>().push(command);
}

fn log(local: &LocalMatch) -> &SessionLog {
    local.server().world().resource::<Session>().log()
}

fn playing_on(local: &LocalMatch) -> bool {
    (0..2).all(|client| {
        let state = local.client(client).world().resource::<JoinState>();
        state.clock().is_some()
    })
}

#[test]
fn a_players_save_is_refused_when_the_mode_alone_saves() {
    let data = Scratch::new("save-by-mode");
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
    let data = Scratch::new("save-load");
    let mut local = playing(&data, SaveBy::Player);

    // The save: a checkpoint at the boundary after the tick that runs next, segment 1.
    command(&mut local, SaveCommand::Save);
    for _ in 0..10 {
        local.step();
    }
    SimServer::settle_checkpoint(local.server_mut().world_mut()).unwrap();
    let save = log(&local).checkpoints().next().unwrap().clone();
    assert_eq!(save.segment, 1);
    for _ in 0..30 {
        local.step();
    }
    assert!(log(&local).next_tick() > save.tick);

    // The quick load: the server starts again on its data, the match at the save's tick and in
    // its state, the log holding nothing after it.
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
    let file = SessionDir::publish(&data.0, log(&local)).unwrap();
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
