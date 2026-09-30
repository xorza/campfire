//! The Stage 2 prototype: the sim schedule in Lightyear's `World`, a server and one predicting
//! client over in-process channels, and the server's session log replayed in a bare `World`
//! with the same state hash after every tick.

use std::num::NonZeroU32;

use bevy_app::App;
use bevy_ecs::entity::Entity;
use campfire_capabilities::{Action, Dead, Destination, Health, Owner, Respawn};
use campfire_math::{Num, Vec3};
use campfire_net::{LocalPair, PlayerLink, TickHashes, Unpredicted};
use campfire_protocol::{SeedChain, SessionLog};
use campfire_runner::{Runner, Session};
use campfire_sim::{EntityIndex, Position, SimTick, Tick};
use lightyear::prelude::{Predicted, PredictionMetrics, RollbackMode};

const SEED_CHAIN: SeedChain = SeedChain::new([9; 32], NonZeroU32::MIN);
/// Frames of match: one tick each.
const MATCH_FRAMES: usize = 120;

fn num(value: i64) -> Num {
    Num::from_int(value).unwrap()
}

fn move_to(x: i64, z: i64) -> Action {
    Action::Move {
        x: num(x),
        z: num(z),
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Hero {
    position: Position,
    destination: Destination,
}

/// The one hero in `app`: the one unit under a player's control.
fn hero_entity(app: &App) -> Entity {
    let world = app.world();
    let mut units = world.resource::<EntityIndex>().iter();
    let (_, entity) = units
        .find(|&(_, entity)| world.entity(entity).contains::<Owner>())
        .unwrap();
    entity
}

fn hero(app: &App) -> Hero {
    let hero = app.world().entity(hero_entity(app));
    Hero {
        position: *hero.get::<Position>().unwrap(),
        destination: *hero.get::<Destination>().unwrap(),
    }
}

#[test]
fn server_and_replay_agree_on_every_tick() {
    // `Check` rolls the client back only on a misprediction; `Always` on every confirmed update,
    // so the client runs the sim again from the server's state many times. A server of 3 frames a
    // tick receives an input in the step's first frame, and runs the tick in one of the three, as
    // the frames it ran alone before shift it: in two of the three, the input arrives in a frame
    // that runs no tick, and must be logged all the same.
    for (rollback, server_frames, shift) in [
        (RollbackMode::Check, 1, 0),
        (RollbackMode::Always, 1, 0),
        (RollbackMode::Check, 3, 0),
        (RollbackMode::Check, 3, 1),
        (RollbackMode::Check, 3, 2),
    ] {
        let case = format!("{rollback:?}, {server_frames} frames, shift {shift}");
        let mut pair = LocalPair::new(rollback, SEED_CHAIN, server_frames);
        for _ in 0..shift {
            pair.server_frame();
        }
        pair.start_match();
        // The ticks the server ran while the client learned the match started.
        let started = pair.server().world().resource::<SimTick>().start().get();
        for frame in 0..MATCH_FRAMES {
            match frame {
                10 => pair.order(move_to(0, 5)),
                50 => pair.order(move_to(-2, 5)),
                _ => {}
            }
            pair.step();
        }

        let arrived = Hero {
            position: Position::new(Vec3::new(num(-2), Num::ZERO, num(5))).unwrap(),
            destination: Destination::default(),
        };
        assert_eq!(hero(pair.server()), arrived, "{case}");
        assert_eq!(hero(pair.client()), arrived, "{case}");
        let client_world = pair.client().world();
        let client_hero = client_world.entity(hero_entity(pair.client()));
        assert!(client_hero.contains::<Predicted>() && !client_hero.contains::<Unpredicted>());
        let rollbacks = client_world.resource::<PredictionMetrics>().rollbacks;
        // The client stamps each order with the tick it predicts it in, and runs ahead of the
        // server, so the server applies it in that tick: nothing is mispredicted.
        match rollback {
            RollbackMode::Check => assert_eq!(rollbacks, 0),
            _ => assert!(rollbacks > 0, "{case} rolled back {rollbacks} times"),
        }

        let link = pair.link();
        let server_world = pair.server_mut().world_mut();
        assert_eq!(
            server_world
                .entity(link)
                .get::<PlayerLink>()
                .unwrap()
                .refused(),
            0
        );
        server_world.resource_mut::<Session>().reveal_seed();
        let live = server_world.resource::<TickHashes>().get().to_vec();
        let ticks = started + u64::try_from(MATCH_FRAMES).unwrap();
        assert_eq!(u64::try_from(live.len()).unwrap(), ticks);
        let mut file = Vec::new();
        server_world.resource::<Session>().log().encode(&mut file);
        let decoded = SessionLog::decode(&file).unwrap();
        let seed = decoded.revealed_seed().unwrap();
        let mut replay = Runner::new(decoded.rewound(), seed, pair.packages()).unwrap();
        for (tick, live) in live.iter().enumerate() {
            replay.run_tick();
            assert_eq!(replay.state_hash(), *live, "{case}, tick {tick}");
        }
        assert_eq!(replay.log().next_tick(), ticks);
    }
}

#[test]
fn a_dead_hero_stays_where_it_died_then_respawns_at_its_spawn_on_the_server_and_its_client() {
    let mut pair = LocalPair::new(RollbackMode::Check, SEED_CHAIN, 1);
    pair.start_match();
    // The hero walks to (4, 0), 4 m from the east tower at (8, 0), which reaches 7.75 m and hits
    // for 150 of its 600: the fourth hit kills it where it stands.
    pair.order(move_to(4, 0));
    let dead = |app: &App| app.world().entity(hero_entity(app)).contains::<Dead>();
    let rollbacks = |pair: &LocalPair| {
        pair.client()
            .world()
            .resource::<PredictionMetrics>()
            .rollbacks
    };
    let mut frames = 0;
    while !dead(pair.server()) {
        assert!(frames < 400, "the tower kills the hero");
        pair.step();
        frames += 1;
    }
    let died_in = pair.server().world().resource::<SimTick>().start().get() - 1;
    // The client learns of the death after the ticks it predicted ahead, and corrects them once.
    for _ in 0..10 {
        pair.step();
    }
    assert_eq!(rollbacks(&pair), 1);
    // An order after its death moves it on neither end, and needs no correction.
    pair.order(move_to(-4, 0));
    for _ in 0..40 {
        pair.step();
    }
    let at = |x| Hero {
        position: Position::new(Vec3::new(num(x), Num::ZERO, Num::ZERO)).unwrap(),
        destination: Destination::default(),
    };
    assert_eq!(hero(pair.server()), at(4));
    assert_eq!(hero(pair.client()), at(4));
    assert!(dead(pair.client()));
    let health = |app: &App| {
        app.world()
            .get::<Health>(hero_entity(app))
            .unwrap()
            .current()
    };
    assert_eq!(health(pair.client()), Num::ZERO);
    assert_eq!(rollbacks(&pair), 1);

    // The lane mode respawns a hero 5000 ms later: 150 ticks at 30 a second, from the end of the
    // tick it died in. It stands at its team's spawn, (0, 0), with its 600 health, on both ends,
    // and the client, which learned the respawn ahead, needs no correction.
    let back = Respawn {
        at: Tick::new(died_in + 1 + 150),
    };
    let client_hero = hero_entity(pair.client());
    assert_eq!(
        pair.client().world().get::<Respawn>(client_hero),
        Some(&back)
    );
    while dead(pair.server()) {
        pair.step();
    }
    let respawned_in = pair.server().world().resource::<SimTick>().start().get() - 1;
    assert_eq!(respawned_in, back.at.get());
    for _ in 0..10 {
        pair.step();
    }
    assert_eq!(hero(pair.server()), at(0));
    assert_eq!(hero(pair.client()), at(0));
    assert!(!dead(pair.client()));
    assert_eq!(health(pair.client()), num(600));
    assert_eq!(rollbacks(&pair), 1);
}
