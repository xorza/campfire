//! The Stage 2 prototype: the sim schedule in Lightyear's `World`, a server and one predicting
//! client over in-process channels, and the server's session log replayed in a bare `World`
//! with the same state hash after every tick.

use std::num::NonZeroU32;

use bevy_app::App;
use bevy_ecs::entity::Entity;
use campfire_capabilities::{Action, Destination, Owner};
use campfire_math::{Num, Vec3};
use campfire_net::{LocalPair, PlayerLink, TickHashes, Unpredicted};
use campfire_protocol::{SeedChain, SessionLog};
use campfire_runner::{Runner, Session};
use campfire_sim::{EntityIndex, Position};
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
    // so the client runs the sim again from the server's state many times.
    for rollback in [RollbackMode::Check, RollbackMode::Always] {
        let mut pair = LocalPair::new(rollback, SEED_CHAIN);
        pair.start_match().unwrap();
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
        assert_eq!(hero(pair.server()), arrived, "{rollback:?}");
        assert_eq!(hero(pair.client()), arrived, "{rollback:?}");
        let client_world = pair.client().world();
        let client_hero = client_world.entity(hero_entity(pair.client()));
        assert!(client_hero.contains::<Predicted>() && !client_hero.contains::<Unpredicted>());
        let rollbacks = client_world.resource::<PredictionMetrics>().rollbacks;
        // The client stamps each order with the tick it predicts it in, and runs ahead of the
        // server, so the server applies it in that tick: nothing is mispredicted.
        match rollback {
            RollbackMode::Check => assert_eq!(rollbacks, 0),
            _ => assert!(rollbacks > 0, "{rollback:?} rolled back {rollbacks} times"),
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
        assert_eq!(live.len(), MATCH_FRAMES);
        let mut file = Vec::new();
        server_world.resource::<Session>().log().encode(&mut file);
        let decoded = SessionLog::decode(&file).unwrap();
        let seed = decoded.revealed_seed().unwrap();
        let mut replay = Runner::new(decoded.rewound(), seed, pair.packages()).unwrap();
        for (tick, live) in live.iter().enumerate() {
            replay.run_tick();
            assert_eq!(replay.state_hash(), *live, "{rollback:?}, tick {tick}");
        }
        assert_eq!(
            replay.log().next_tick(),
            u64::try_from(MATCH_FRAMES).unwrap()
        );
    }
}
