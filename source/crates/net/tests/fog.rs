//! Fog of war over the network: a client receives only the units its team sees.

use std::num::NonZeroU32;

use bevy_app::App;
use bevy_ecs::entity::Entity;
use campfire_capabilities::{Action, Owner, SeenBy, Team};
use campfire_math::{Num, Vec3};
use campfire_net::{LocalPair, MatchClock, TickHashes, Unpredicted};
use campfire_protocol::SeedChain;
use campfire_sim::{EntityIndex, Position, SimTick, StableId, Tick};
use lightyear::prelude::{ConfirmHistory, ReplicationCheckpointMap, RollbackMode};

const SEED_CHAIN: SeedChain = SeedChain::new([9; 32], NonZeroU32::MIN);
/// Frames of match: one tick each.
const MATCH_FRAMES: usize = 90;

fn num(value: i64) -> Num {
    Num::from_int(value).unwrap()
}

fn move_to(x: i64) -> Action {
    Action::Move {
        x: num(x),
        z: Num::ZERO,
    }
}

/// The unit of `app` that `pick` finds first, in stable id order.
fn unit(app: &App, pick: impl Fn(&App, Entity) -> bool) -> (StableId, Entity) {
    let mut units = app.world().resource::<EntityIndex>().iter();
    units.find(|&(_, entity)| pick(app, entity)).unwrap()
}

/// What the server's last tick left: the west team sees the east tower, and the hero's x.
#[derive(Debug, Clone, Copy)]
struct Server {
    seen: bool,
    hero_x: Num,
}

#[test]
fn an_enemy_reaches_the_client_in_the_tick_it_comes_into_sight() {
    let mut pair = LocalPair::new(RollbackMode::Check, SEED_CHAIN);
    pair.start_match().unwrap();
    // Without the per-tick hash, as in production: the ticks and the replication run all the same.
    let hashes = pair
        .server_mut()
        .world_mut()
        .remove_resource::<TickHashes>();
    assert_eq!(hashes.map(|hashes| hashes.get().len()), Some(0));
    let east_tower = Position::new(Vec3::new(num(8), Num::ZERO, Num::ZERO)).unwrap();
    let (tower, tower_entity) = unit(pair.server(), |app, entity| {
        app.world().get::<Position>(entity) == Some(&east_tower)
    });
    let (_, hero) = unit(pair.server(), |app, entity| {
        app.world().entity(entity).contains::<Owner>()
    });

    // By sim tick, from 0.
    let mut server = Vec::new();
    // By frame: the client holds the tower.
    let mut on_client = Vec::new();
    let mut arrived_in = None;
    for frame in 0..MATCH_FRAMES {
        match frame {
            10 => pair.order(move_to(4)),
            40 => pair.order(move_to(0)),
            _ => {}
        }
        pair.step();
        let world = pair.server().world();
        let ticks_run = world.resource::<SimTick>().start().get();
        if ticks_run > u64::try_from(server.len()).unwrap() {
            let seen = world.get::<SeenBy>(tower_entity).unwrap().get();
            server.push(Server {
                seen: seen.contains(Team::new(0)),
                hero_x: world.get::<Position>(hero).unwrap().get().x,
            });
        }
        let client = pair.client().world();
        let held = client.resource::<EntityIndex>().get(tower);
        if let (Some(entity), None) = (held, arrived_in) {
            assert!(client.entity(entity).contains::<Unpredicted>());
            arrived_in = Some(sent_in(&pair, entity));
        }
        on_client.push(held.is_some());
    }

    // The hero walks ¼ m a tick from (0, 0) and sees 6 m. The tower's cell spans x 8 to 9 and z 0
    // to 1, its center (8.5, 0.5): from x = 2.5 it is √(6² + 0.5²) ≈ 6.02 m away, hidden; from
    // x = 2.75, √(5.75² + 0.5²) ≈ 5.77 m, seen. Then the hero walks back, out of sight.
    let hidden_at = Num::from_bits(10 << (Num::FRAC_BITS - 2));
    let seen_at = Num::from_bits(11 << (Num::FRAC_BITS - 2));
    let first = server.iter().position(|tick| tick.seen).unwrap();
    let last = server.iter().rposition(|tick| tick.seen).unwrap();
    assert_eq!(server[first - 1].hero_x, hidden_at);
    assert_eq!(server[first].hero_x, seen_at);
    assert_eq!(server[last].hero_x, seen_at);
    assert_eq!(server[last + 1].hero_x, hidden_at);
    assert!(server[first..=last].iter().all(|tick| tick.seen));

    // The client holds the tower from the message of the tick it came into sight in, and not
    // before, and has dropped it by the end.
    assert_eq!(arrived_in, Some(Tick::new(u64::try_from(first).unwrap())));
    let held_from = on_client.iter().position(|&held| held).unwrap();
    let held_until = on_client.iter().rposition(|&held| held).unwrap();
    assert!(on_client[held_from..=held_until].iter().all(|&held| held));
    assert!(held_until < MATCH_FRAMES - 1);
}

/// The sim tick of the server message that last updated `unit` on the client.
fn sent_in(pair: &LocalPair, unit: Entity) -> Tick {
    let client = pair.client().world();
    let history = client.get::<ConfirmHistory>(unit).unwrap();
    let net_tick = client
        .resource::<ReplicationCheckpointMap>()
        .get(history.last_tick())
        .unwrap();
    let clock = pair.server().world().resource::<MatchClock>();
    clock.sim_tick(net_tick).unwrap()
}
