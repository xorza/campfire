//! The Stage 2 prototype: the sim schedule in Lightyear's `World`, a server and one predicting
//! client over in-process channels, and the server's session log replayed in a bare `World`
//! with the same state hash after every tick.

use bevy_app::App;
use campfire_kit_moba::{Destination, Order};
use campfire_math::{Num, Vec3};
use campfire_net::{LocalPair, PlayerLink, TickHashes};
use campfire_protocol::ServerSeed;
use campfire_runner::Session;
use campfire_sim::{EntityIndex, Position};
use campfire_verifier::Replay;
use lightyear::prelude::{Predicted, PredictionMetrics, RollbackMode};

const SERVER_SEED: ServerSeed = ServerSeed::new([9; 32]);
/// Frames of match: one tick each.
const MATCH_FRAMES: usize = 120;

fn num(value: i64) -> Num {
    Num::from_int(value).unwrap()
}

fn move_to(x: i64, z: i64) -> Order {
    Order::Move {
        x: num(x),
        z: num(z),
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Hero {
    position: Position,
    destination: Destination,
}

/// The one hero in `app`.
fn hero(app: &App) -> Hero {
    let world = app.world();
    let (_, entity) = world.resource::<EntityIndex>().iter().next().unwrap();
    let hero = world.entity(entity);
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
        let mut pair = LocalPair::new(rollback);
        pair.start_match(SERVER_SEED).unwrap();
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
        let (_, client_hero) = client_world
            .resource::<EntityIndex>()
            .iter()
            .next()
            .unwrap();
        assert!(client_world.entity(client_hero).contains::<Predicted>());
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
        let live = server_world.resource::<TickHashes>().get();
        assert_eq!(live.len(), MATCH_FRAMES);
        let mut replay = Replay::new(server_world.resource::<Session>().log()).unwrap();
        for (tick, live) in live.iter().enumerate() {
            replay.next_tick().unwrap().unwrap();
            assert_eq!(
                replay.runner().state_hash(),
                *live,
                "{rollback:?}, tick {tick}"
            );
        }
        assert!(replay.next_tick().is_none());
    }
}
