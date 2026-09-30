use bevy_ecs::entity::Entity;
use campfire_math::{Num, SegmentSeed};
use campfire_sim::{EntityIndex, SimUpdate, TickInput};

use super::*;

const ONE: i64 = 1 << 24;

fn num(value: i64) -> Num {
    Num::from_int(value).unwrap()
}

fn at(x: i64, y: i64, z: i64) -> Position {
    Position::new(Vec3::new(num(x), num(y), num(z))).unwrap()
}

fn raw(x: i64, y: i64, z: i64) -> Position {
    Position::new(Vec3::new(
        Num::from_bits(x),
        Num::from_bits(y),
        Num::from_bits(z),
    ))
    .unwrap()
}

#[derive(Debug, PartialEq, Eq)]
struct Hero {
    position: Position,
    destination: Option<Position>,
}

/// Where a hero is and walks to.
const fn hero(position: Position, destination: Option<Position>) -> Hero {
    Hero {
        position,
        destination,
    }
}

/// Heroes of slots 0 and 1, one meter a tick: slot 0 at the origin, slot 1 at x = 4 and y = 2.
#[derive(Debug)]
struct Match {
    world: World,
    heroes: [Entity; 2],
}

impl Match {
    fn new() -> Match {
        let mut world = World::new();
        SimUpdate::prepare(&mut world, SegmentSeed::new([0; 32]));
        let step = MoveStep::new(Num::ONE).unwrap();
        let heroes = [(0, at(0, 0, 0)), (1, at(4, 2, 0))].map(|(slot, position)| {
            let id = MobaKit::spawn_hero(&mut world, slot, position, step);
            world.resource::<EntityIndex>().get(id).unwrap()
        });
        let mut schedule = SimUpdate::schedule();
        MobaKit::add_systems(&mut schedule);
        world.add_schedule(schedule);
        Match { world, heroes }
    }

    fn tick(&mut self, inputs: &[(u32, &[u8])]) {
        let mut tick_inputs = self.world.resource_mut::<TickInputs>();
        for &(slot, payload) in inputs {
            tick_inputs.push(TickInput { slot, payload });
        }
        self.world.run_schedule(SimUpdate);
    }

    fn hero(&self, slot: usize) -> Hero {
        let hero = self.world.entity(self.heroes[slot]);
        Hero {
            position: *hero.get::<Position>().unwrap(),
            destination: hero.get::<Destination>().unwrap().get(),
        }
    }
}

fn move_to(x: i64, z: i64) -> Vec<u8> {
    Order::Move {
        x: num(x),
        z: num(z),
    }
    .encode()
}

#[test]
fn orders_decode_exactly() {
    let order = Order::Move {
        x: num(-3),
        z: Num::from_bits(5),
    };
    let payload = order.encode();
    assert_eq!(Order::decode(&payload), Some(order));
    assert_eq!(Order::decode(&[payload.as_slice(), &[0]].concat()), None);
    assert_eq!(Order::decode(&payload[..payload.len() - 1]), None);
    assert_eq!(Order::decode(&[]), None);
    // Variant 1 does not exist.
    assert_eq!(Order::decode(&[1, 0, 0]), None);
}

#[test]
fn a_hero_walks_to_its_players_target() {
    let mut game = Match::new();
    game.tick(&[(0, &move_to(0, 5))]);
    // Straight along z, one meter a tick: exact, and done in the fifth tick.
    assert_eq!(game.hero(0), hero(at(0, 0, 1), Some(at(0, 0, 5))));
    assert_eq!(game.hero(1), hero(at(4, 2, 0), None));
    for z in 2..5 {
        game.tick(&[]);
        assert_eq!(game.hero(0), hero(at(0, 0, z), Some(at(0, 0, 5))));
    }
    game.tick(&[]);
    assert_eq!(game.hero(0), hero(at(0, 0, 5), None));
    game.tick(&[]);
    assert_eq!(game.hero(0), hero(at(0, 0, 5), None));

    // Offset (3, 0, 4) from slot 1's hero, distance 5: the first meter is 3/5 and 4/5, each
    // rounded once: 10 066 329.6 → 10 066 330 and 13 421 772.8 → 13 421 773. The order keeps
    // the hero's height, y = 2.
    game.tick(&[(1, &move_to(7, 4))]);
    assert_eq!(
        game.hero(1),
        hero(
            raw(4 * ONE + 10_066_330, 2 * ONE, 13_421_773),
            Some(at(7, 2, 4))
        )
    );
    assert_eq!(game.hero(0), hero(at(0, 0, 5), None));
}

#[test]
fn the_last_order_in_a_tick_wins_and_bad_ones_are_ignored() {
    let mut game = Match::new();
    game.tick(&[(0, &move_to(0, 5)), (0, &move_to(0, -5))]);
    assert_eq!(game.hero(0), hero(at(0, 0, -1), Some(at(0, 0, -5))));

    let beyond = Order::Move {
        x: Position::BOUND + Num::EPSILON,
        z: Num::ZERO,
    }
    .encode();
    let edge = Order::Move {
        x: Position::BOUND,
        z: Num::ZERO,
    }
    .encode();
    game.tick(&[
        (0, &beyond),
        (0, b"not an order"),
        (2, &move_to(9, 9)),
        (1, &edge),
    ]);
    assert_eq!(game.hero(0), hero(at(0, 0, -2), Some(at(0, 0, -5))));
    assert_eq!(
        game.hero(1).destination,
        Some(Position::new(Vec3::new(Position::BOUND, num(2), Num::ZERO)).unwrap())
    );
}

#[test]
fn move_steps_are_never_negative() {
    assert_eq!(MoveStep::new(-Num::EPSILON), None);
    assert_eq!(MoveStep::new(Num::ZERO).map(MoveStep::get), Some(Num::ZERO));
    let negative = postcard::to_allocvec(&-Num::EPSILON).unwrap();
    assert!(postcard::from_bytes::<MoveStep>(&negative).is_err());
    let one = postcard::to_allocvec(&Num::ONE).unwrap();
    assert_eq!(
        postcard::from_bytes::<MoveStep>(&one).ok(),
        MoveStep::new(Num::ONE)
    );
}
