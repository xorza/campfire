//! Fog of war over the network: a client receives only the units its team sees.

use std::num::NonZeroU32;

use bevy_app::App;
use bevy_ecs::entity::Entity;
use campfire_capabilities::{Action, Owner, SeenBy, Team};
use campfire_math::{Num, Tick, Vec3};
use campfire_net::{LocalMatch, MatchClock, MatchSetup, TickHashes};
use campfire_protocol::SeedChain;
use campfire_sim::{EntityIndex, Position, SimTick, StableId, Unpredicted};
use lightyear::prelude::{ConfirmHistory, ReplicationCheckpointMap, RollbackMode};

const SEED_CHAIN: SeedChain = SeedChain::new([9; 32], NonZeroU32::MIN);
/// Frames of match: one tick each.
const MATCH_FRAMES: usize = 90;

fn num(value: i64) -> Num {
    Num::from_int(value).unwrap()
}

/// A move along the hero's line from its spawn, z = −2.
fn move_to(x: i64) -> Action {
    Action::Move {
        x: num(x),
        z: num(-2),
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
    let mut local = LocalMatch::new(MatchSetup::solo(RollbackMode::Check, 1, SEED_CHAIN));
    local.start_match();
    // Without the per-tick hash from here on, as in production: the ticks and the replication run
    // all the same.
    let hashes = local
        .server_mut()
        .world_mut()
        .remove_resource::<TickHashes>();
    assert!(hashes.is_some());
    let east_tower = Position::new(Vec3::new(num(8), Num::ZERO, num(-3))).unwrap();
    let (tower, tower_entity) = unit(local.server(), |app, entity| {
        app.world().get::<Position>(entity) == Some(&east_tower)
    });
    let (_, hero) = unit(local.server(), |app, entity| {
        app.world().entity(entity).contains::<Owner>()
    });

    // By sim tick, from 0; `None` for the ticks the lobby ran before the loop.
    let mut server: Vec<Option<Server>> = Vec::new();
    // By frame: the client holds the tower.
    let mut on_client = Vec::new();
    let mut arrived_in = None;
    for frame in 0..MATCH_FRAMES {
        match frame {
            10 => local.order(0, move_to(4)),
            40 => local.order(0, move_to(0)),
            _ => {}
        }
        local.step();
        let world = local.server().world();
        let ticks_run = usize::try_from(world.resource::<SimTick>().start().get()).unwrap();
        if ticks_run > server.len() {
            let seen = world.get::<SeenBy>(tower_entity).unwrap().get();
            server.resize(ticks_run - 1, None);
            server.push(Some(Server {
                seen: seen.contains(Team::new(0)),
                hero_x: world.get::<Position>(hero).unwrap().get().x,
            }));
        }
        let client = local.client(0).world();
        let held = client.resource::<EntityIndex>().get(tower);
        if let (Some(entity), None) = (held, arrived_in) {
            assert!(client.entity(entity).contains::<Unpredicted>());
            arrived_in = Some(sent_in(&local, entity));
        }
        on_client.push(held.is_some());
    }

    // The hero walks ¼ m a tick from (0, −2) and sees 6 m. The tower's cell spans x 8 to 9 and z
    // −3 to −2, its center (8.5, −2.5): from x = 2.5 it is √(6² + 0.5²) ≈ 6.02 m away, hidden;
    // from x = 2.75, √(5.75² + 0.5²) ≈ 5.77 m, seen. Then the hero walks back, out of sight.
    let hidden_at = Num::from_bits(10 << (Num::FRAC_BITS - 2));
    let seen_at = Num::from_bits(11 << (Num::FRAC_BITS - 2));
    let seen = |tick: &Option<Server>| tick.is_some_and(|tick| tick.seen);
    let first = server.iter().position(seen).unwrap();
    let last = server.iter().rposition(seen).unwrap();
    let hero_x = |tick: usize| server[tick].unwrap().hero_x;
    assert_eq!(hero_x(first - 1), hidden_at);
    assert_eq!(hero_x(first), seen_at);
    assert_eq!(hero_x(last), seen_at);
    assert_eq!(hero_x(last + 1), hidden_at);
    assert!(server[first..=last].iter().all(seen));

    // The client holds the tower from the message of the tick it came into sight in, and not
    // before, and has dropped it by the end.
    assert_eq!(arrived_in, Some(Tick::new(u64::try_from(first).unwrap())));
    let held_from = on_client.iter().position(|&held| held).unwrap();
    let held_until = on_client.iter().rposition(|&held| held).unwrap();
    assert!(on_client[held_from..=held_until].iter().all(|&held| held));
    assert!(held_until < MATCH_FRAMES - 1);
}

/// The sim tick of the server message that last updated `unit` on the client.
fn sent_in(local: &LocalMatch, unit: Entity) -> Tick {
    let client = local.client(0).world();
    let history = client.get::<ConfirmHistory>(unit).unwrap();
    let net_tick = client
        .resource::<ReplicationCheckpointMap>()
        .get(history.last_tick())
        .unwrap();
    let clock = local.server().world().resource::<MatchClock>();
    clock.sim_tick(net_tick).unwrap()
}
