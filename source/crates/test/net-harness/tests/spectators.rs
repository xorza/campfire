//! Spectators (10-replication-audiences.md, The host admits spectators, and their fog is a
//! setting): a link with no slot, admitted while the session holds a place for it, watches every
//! unit through any fog, or the teams' views it chooses, and holds of each entity what its kind's
//! audience gives spectators. It predicts nothing, and its join's burst of the whole world crosses
//! a slow link in the frames its rate allows. A watched session may seat bots alone, and starts as
//! its watcher joins.

use std::collections::BTreeSet;

use bevy_app::{App, AppExit};
use bevy_ecs::query::{Allow, With};
use campfire_bots::OrderScript;
use campfire_net::{
    CloseCode, JoinError, JoinRefused, JoinState, LinkLossCause, LinkLost, LobbyStart, Loss,
    MatchClock, SimServerWorld, StopRequest, WatchRequest,
};
use campfire_net_core::LinkDirection;
use campfire_net_emulator::{LinkModel, RateLimit};
use campfire_net_harness::{End, InProcessMatch, InProcessSetup};
use campfire_net_prediction::RollbackMode;
use campfire_net_replication::Replicate;
use campfire_progression::Experience;
use campfire_sim::{EntityIndex, StableId, Unpredicted};
use campfire_store::internals::TestDir;
use campfire_units::{Life, SpawnPoint, UnitsWorld};
use campfire_values::{Relations, Team, TeamSet};

use crate::short_3v3::{PICKS, short_3v3};

/// Steps a spectator may take to link, answer the offer and watch.
const JOIN_STEPS: usize = 300;

/// A duo of the lane mode whose host gives `visibility` and admits `spectators`.
fn duo(visibility: &'static [&'static str], spectators: u32) -> InProcessSetup {
    InProcessSetup {
        visibility,
        max_spectators: spectators,
        ..InProcessSetup::duo(LinkModel::perfect(), InProcessMatch::SEED_CHAIN)
    }
}

fn state(local: &InProcessMatch, client: usize) -> &JoinState {
    local.client(client).world().resource::<JoinState>()
}

/// The stable ids of the entities `app` holds, predicted or not.
fn held(app: &mut App) -> BTreeSet<StableId> {
    let world = app.world_mut();
    let mut ids = world.query_filtered::<&StableId, Allow<Unpredicted>>();
    ids.iter(world).copied().collect()
}

/// The stable ids of the entities the server replicates.
fn replicated(app: &mut App) -> BTreeSet<StableId> {
    let world = app.world_mut();
    let mut ids = world.query_filtered::<&StableId, With<Replicate>>();
    ids.iter(world).copied().collect()
}

/// A spectator that joins `local` and steps until it watches; its index.
fn watching(local: &mut InProcessMatch) -> usize {
    let spectator = local.add_spectator();
    local.step_until(JOIN_STEPS, |local| {
        state(local, spectator).watching_clock().is_some()
    });
    for _ in 0..5 {
        local.step();
    }
    spectator
}

#[test]
fn a_spectator_watches_every_unit_holds_what_spectators_may_and_only_one_finds_a_place() {
    let mut local = InProcessMatch::new(duo(&[], 1));
    local.start_match();
    let spectator = watching(&mut local);

    // Every entity the server replicates, a unit no team sees among them, as the west player,
    // who sees less, shows; and nothing it predicts.
    let all = replicated(local.server_mut());
    let watched = held(local.client_mut(spectator));
    assert_eq!(watched, all);
    let west = held(local.client_mut(0));
    assert!(west.is_subset(&all) && west.len() < all.len(), "{west:?}");
    let world = local.client_mut(spectator).world_mut();
    assert_eq!(world.query::<&StableId>().iter(world).count(), 0);

    // A hero's life, which every client that holds it receives, and not its spawn point, its
    // owner's alone; the teams' relations, on the match's entity.
    let hero = local.avatar(0);
    let world = local.client(spectator).world();
    let hero = world.entity(world.resource::<EntityIndex>().get(hero).unwrap());
    assert!(hero.contains::<Life>() && !hero.contains::<SpawnPoint>());
    let relations = world.match_entity().map(|entity| world.entity(entity));
    assert!(relations.is_some_and(|entity| entity.contains::<Relations>()));

    // The one place is taken: a second spectator is refused, told, and stops.
    let second = local.add_spectator();
    local.step_until(JOIN_STEPS, |local| state(local, second).loss().is_some());
    assert_eq!(state(&local, second).loss(), Some(Loss::JoinRefused));
    let refused: Vec<JoinError> = local
        .log()
        .take::<JoinRefused>()
        .iter()
        .map(|refused| refused.error)
        .collect();
    assert_eq!(refused, [JoinError::NoSpectatorPlace]);
    assert_eq!(
        local.log().take::<LinkLost>(),
        [LinkLost {
            cause: LinkLossCause::ClientClosed(CloseCode::JoinRefused)
        }]
    );
}

#[test]
fn a_watched_session_of_bots_alone_starts_as_its_watcher_joins_and_runs_on_until_a_stop() {
    // Two bots, one a side, each picking its hero in tick 0, and no player
    // (12-test-matches.md, A local session may seat bots alone).
    const PICK: &str = "[[input]]\ntick = 0\nname = \"pick\"\nvalue = \"runner\"\n";
    let test_dir = TestDir::new();
    let mut local = InProcessMatch::new(InProcessSetup {
        players: 0,
        bots: &[PICK, PICK],
        start: LobbyStart::Watched,
        ..duo(&[], 1)
    });
    local.keep_data(test_dir.path("data"));
    local.open_lobby();
    for _ in 0..JOIN_STEPS {
        local.step();
    }
    assert!(!local.server().world().contains_resource::<MatchClock>());

    // The match starts as the watcher joins, which watches as the server played tick 0 alone,
    // and holds every entity, both sides' heroes among them.
    let spectator = local.add_spectator();
    local.step_until(JOIN_STEPS, |local| {
        state(local, spectator).watching_clock().is_some()
    });
    assert_eq!(local.next_tick(End::Server), 1);
    for _ in 0..5 {
        local.step();
    }
    let all = replicated(local.server_mut());
    assert_eq!(held(local.client_mut(spectator)), all);
    let world = local.server_mut().world_mut();
    let mut heroes = world.query_filtered::<&Team, With<Experience>>();
    let teams: BTreeSet<Team> = heroes.iter(world).copied().collect();
    assert_eq!(teams.len(), 2);

    // No player is left to leave: the session runs on until a stop.
    for _ in 0..30 {
        local.step();
    }
    let world = local.server_mut().world_mut();
    assert_eq!(world.exit_due(StopRequest::NotAsked), None);
    assert_eq!(world.exit_due(StopRequest::Asked), Some(AppExit::Success));
}

#[test]
fn a_spectator_of_the_teams_views_watches_the_teams_it_chooses() {
    // Every team's view together, as it joins; then the west's alone, as the west player sees
    // it.
    let mut local = InProcessMatch::new(duo(&["spectators=teams"], 1));
    local.start_match();
    let spectator = watching(&mut local);
    let west = held(local.client_mut(0));
    let east = held(local.client_mut(1));
    let both: BTreeSet<StableId> = west.union(&east).copied().collect();
    assert_eq!(held(local.client_mut(spectator)), both);
    assert_ne!(west, both, "the teams see apart");

    let west_alone = TeamSet::from([Team::new(0)]);
    local
        .client_mut(spectator)
        .world_mut()
        .insert_resource(WatchRequest(west_alone));
    for _ in 0..5 {
        local.step();
    }
    assert_eq!(held(local.client_mut(spectator)), held(local.client_mut(0)));

    // Its link fails and comes back: the place its old link held is free again, and it watches
    // the west's view alone again, as it chose.
    local.cut_link(spectator);
    local.step_until(JOIN_STEPS, |local| state(local, spectator).is_rejoining());
    local.mend_link(spectator);
    local.step_until(JOIN_STEPS, |local| {
        state(local, spectator).watching_clock().is_some()
    });
    for _ in 0..5 {
        local.step();
    }
    assert_eq!(held(local.client_mut(spectator)), held(local.client_mut(0)));
}

/// What a spectator's join to the short 3v3 ten seconds in cost its link, by server frame from
/// the join, until it held every entity the server replicates.
#[derive(Debug, PartialEq, Eq)]
struct JoinBurst {
    /// The bytes the server put on the spectator's wire in each frame.
    frames: Vec<u64>,
    /// The entities it then held.
    entities: usize,
}

impl JoinBurst {
    /// The join over links of `link`, the server running a frame a tick.
    fn over(link: LinkModel) -> Self {
        let mut setup = InProcessSetup::solo(RollbackMode::Check, 1, InProcessMatch::SEED_CHAIN);
        setup.bots = &PICKS[1..];
        setup.max_spectators = 1;
        setup.link = link;
        let mut local = InProcessMatch::of_mode(setup, short_3v3(&[], ""));
        local.open_match();
        local.play(0, &OrderScript::parse(PICKS[0]).unwrap());
        local.await_avatars(JOIN_STEPS);
        for _ in 0..300 {
            local.step();
        }
        let spectator = local.add_spectator();
        let sent = |local: &InProcessMatch| {
            local
                .wire_counts(spectator, LinkDirection::ServerToClient)
                .sent_bytes
        };
        let mut frames = Vec::new();
        for _ in 0..JOIN_STEPS {
            let before = sent(&local);
            local.step();
            frames.push(sent(&local) - before);
            let all = replicated(local.server_mut());
            if state(&local, spectator).watching_clock().is_some()
                && held(local.client_mut(spectator)) == all
            {
                return Self {
                    frames,
                    entities: all.len(),
                };
            }
        }
        panic!("the spectator held every entity within {JOIN_STEPS} steps");
    }

    fn largest(&self) -> u64 {
        self.frames.iter().copied().max().unwrap()
    }
}

/// A link whose queue holds a second of its rate, `bytes_per_second`, with no delay and no loss.
const fn rated(bytes_per_second: u32) -> LinkModel {
    LinkModel::perfect().with_rate(RateLimit::new(bytes_per_second, bytes_per_second))
}

#[test]
fn a_join_burst_fits_the_initial_window_and_a_megabit_link_delays_it_one_tick() {
    // At 10 Mbit/s each datagram arrives at the other end's next frame, as the link takes time to
    // carry it: the handshake's frames, then the world's 58 entities in one frame of 4,431 bytes,
    // the largest, which takes 3.6 ms. It is below NewReno's initial window of 10 packets of
    // 1,200 bytes, so the packet session's budget sends it whole (11-net-stack/packet-session.md,
    // Congestion control is NewReno, in the packet session).
    let fast = JoinBurst::over(rated(1_250_000));
    assert_eq!(fast.frames, [0, 43, 0, 431, 0, 600, 46, 4_431, 417]);
    assert_eq!(fast.entities, 58);
    assert!(fast.largest() < 10 * 1_200);

    // At 1 Mbit/s the 4,431 bytes take 35,448 µs, past the tick of 33,333 µs: the spectator
    // holds the world one frame later, in which the server sent one more frame's bytes.
    let slow = JoinBurst::over(rated(125_000));
    assert_eq!(slow.frames, [0, 43, 0, 431, 0, 600, 46, 4_431, 417, 429]);
    assert_eq!(slow.entities, 58);
    assert_eq!(slow.largest() * 1_000_000 / 125_000, 35_448);
}
