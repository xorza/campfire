use std::num::NonZeroU32;

use bevy_ecs::component::Component;
use campfire_sim::{Capability, EntityIndex, SimUpdate, Tick, TypeHash};

use super::*;
use crate::capability_set::internals::TestMatch;
use crate::mode::map_data::{GridData, NeutralSpawnData, PathData, StructureData};
use crate::navigation::path_walker::PathDirection;
use crate::stats::unit_state::UnitState;
use crate::units::path_id::PathId;
use crate::values::scalar::Scalar;

/// The MOBA's 30 ticks a second.
const RATE: TickRate = TickRate::new(NonZeroU32::new(30).unwrap());

const ONE: i64 = 1 << 24;

fn num(value: i64) -> Num {
    Num::from_int(value).unwrap()
}

fn at(x: i64, y: i64, z: i64) -> Position {
    Position::new(Vec3::new(num(x), num(y), num(z))).unwrap()
}

#[derive(Debug)]
struct Walk {
    world: World,
    registry: StateRegistry,
}

impl Walk {
    fn new() -> Walk {
        let TestMatch {
            mut world,
            schedule,
            registry,
        } = TestMatch::new(&[Capability::Navigation], RATE, None);
        world.add_schedule(schedule);
        Walk { world, registry }
    }

    /// A unit at `at` walking a meter a tick to `to`.
    fn unit(&mut self, at: Position, to: Option<Position>) -> StableId {
        let id = self.world.resource_mut::<IdAllocator>().allocate();
        let entity = self
            .world
            .spawn((id, at, MoveStep::new(Num::ONE).unwrap().bundle()))
            .id();
        self.world.get_mut::<Destination>(entity).unwrap().set(to);
        id
    }

    /// A unit at `at` with a body of `radius`, walking `step` a tick to `to`, or one that does not
    /// walk when `step` is `None`.
    fn body(
        &mut self,
        at: Position,
        to: Option<Position>,
        step: Option<Num>,
        radius: Num,
    ) -> StableId {
        let id = self.world.resource_mut::<IdAllocator>().allocate();
        let mut unit = self.world.spawn((id, at, Body::new(radius).unwrap()));
        if let Some(step) = step {
            unit.insert(MoveStep::new(step).unwrap().bundle());
            unit.get_mut::<Destination>().unwrap().set(to);
        }
        id
    }

    /// Puts unit `id` in `states`, as its modifiers would.
    fn set_states(&mut self, id: StableId, states: &[UnitState]) {
        let entity = self.world.resource::<EntityIndex>().get(id).unwrap();
        self.world
            .entity_mut(entity)
            .insert(UnitStats::in_states(states));
    }

    fn get<C: Component + Copy>(&self, id: StableId) -> C {
        let entity = self.world.resource::<EntityIndex>().get(id).unwrap();
        *self.world.entity(entity).get::<C>().unwrap()
    }

    fn tick(&mut self) {
        self.world.run_schedule(SimUpdate);
    }
}

#[test]
fn a_unit_walks_to_its_destination_exactly() {
    let mut walk = Walk::new();
    let straight = walk.unit(at(0, 0, 0), Some(at(0, 0, 5)));
    // Offset (3, 0, 4), distance 5: the first meter is 3/5 and 4/5, each rounded once:
    // 10 066 329.6 → 10 066 330 and 13 421 772.8 → 13 421 773. The height stays at y = 2.
    let slanted = walk.unit(at(4, 2, 0), Some(at(7, 2, 4)));
    let still = walk.unit(at(9, 0, 9), None);

    walk.tick();
    assert_eq!(walk.get::<Position>(straight), at(0, 0, 1));
    assert_eq!(
        walk.get::<Position>(slanted),
        Position::new(Vec3::new(
            Num::from_bits(4 * ONE + 10_066_330),
            num(2),
            Num::from_bits(13_421_773)
        ))
        .unwrap()
    );
    assert_eq!(walk.get::<Position>(still), at(9, 0, 9));
    // Straight along z, one meter a tick: done in the fifth tick, and the destination dropped.
    for z in 2..=5 {
        walk.tick();
        assert_eq!(walk.get::<Position>(straight), at(0, 0, z));
    }
    assert_eq!(walk.get::<Destination>(straight).get(), None);
    walk.tick();
    assert_eq!(walk.get::<Position>(straight), at(0, 0, 5));
}

#[test]
fn every_unit_that_walks_ends_the_tick_within_the_bounds() {
    let mut walk = Walk::new();
    assert_eq!(*walk.world.resource::<Bounds>(), Bounds::WORLD);
    walk.world
        .insert_resource(Bounds::new([num(-4), num(-4)], [num(4), num(4)]).unwrap());
    // Outside the bounds at x = 6: clamped to the edge x = 4, at its height and its z.
    let outside = walk.unit(at(6, 1, 2), None);
    // From x = 3 a meter towards x = 5: it arrives at 4, then walks on to 5 and back to 4.
    let past = walk.unit(at(3, 0, 0), Some(at(5, 0, 0)));
    let inside = walk.unit(at(1, 0, 1), None);
    let id = walk.world.resource_mut::<IdAllocator>().allocate();
    walk.world.spawn((id, at(9, 0, 9)));

    walk.tick();
    assert_eq!(walk.get::<Position>(outside), at(4, 1, 2));
    assert_eq!(walk.get::<Position>(past), at(4, 0, 0));
    assert_eq!(walk.get::<Position>(inside), at(1, 0, 1));
    walk.tick();
    assert_eq!(walk.get::<Position>(past), at(4, 0, 0));
    assert_eq!(walk.get::<Destination>(past).get(), None);
    // A unit that does not walk is not moved: the map's check keeps it within the bounds.
    assert_eq!(walk.get::<Position>(id), at(9, 0, 9));
}

#[test]
fn bodies_part_and_block_the_way() {
    // Bodies of 0.5 m walking a quarter meter a tick. A from x = 0 and B from x = 3 walk at each
    // other: 3 − 0.5 t apart after tick t, and in tick 5, at 1.25 and 1.75, they overlap by half
    // a meter, which parts them a quarter meter each, to 1 and 2: 1 m apart, the sum of their
    // radii. Every tick after, they walk in and part again, to the same places.
    let quarter = Num::from_bits(1 << 22);
    let half = Num::from_bits(1 << 23);
    let mut walk = Walk::new();
    let a = walk.body(at(0, 0, 0), Some(at(10, 0, 0)), Some(quarter), half);
    let b = walk.body(at(3, 0, 0), Some(at(-10, 0, 0)), Some(quarter), half);
    // A tower of radius 1 at (−6, 4), which does not walk, and C walking at it along z from 0: C
    // touches it at z = 2.5, 1.5 m off, and in tick 11, at 2.75, is pushed back all the way.
    let tower = walk.body(at(-6, 0, 4), None, None, Num::ONE);
    let c = walk.body(at(-6, 0, 0), Some(at(-6, 0, 8)), Some(quarter), half);
    // D walks through a dead body on its way.
    let dead = walk.body(at(20, 0, 0), None, Some(quarter), half);
    let entity = walk.world.resource::<EntityIndex>().get(dead).unwrap();
    walk.world.entity_mut(entity).insert(Dead);
    let d = walk.body(at(18, 0, 0), Some(at(22, 0, 0)), Some(quarter), half);
    // A walker at a rooted unit on its way along z = 10, which so stands: the walker touches it
    // in tick 8, at x = 1, and from tick 9 takes the whole overlap, back to 1 each tick, as a
    // walker takes it from a unit that stands. The rooted unit keeps its place and destination.
    let rooted = walk.body(at(0, 0, 10), Some(at(10, 0, 10)), Some(quarter), half);
    walk.set_states(rooted, &[UnitState::Rooted]);
    let walker = walk.body(at(3, 0, 10), Some(at(-10, 0, 10)), Some(quarter), half);
    for _ in 0..5 {
        walk.tick();
    }
    assert_eq!(
        [a, b].map(|unit| walk.get::<Position>(unit)),
        [at(1, 0, 0), at(2, 0, 0)]
    );
    for _ in 5..16 {
        walk.tick();
    }
    assert_eq!(
        [a, b].map(|unit| walk.get::<Position>(unit)),
        [at(1, 0, 0), at(2, 0, 0)]
    );
    let c_at = Position::new(Vec3::new(num(-6), Num::ZERO, num(2) + half)).unwrap();
    assert_eq!(
        [c, tower].map(|unit| walk.get::<Position>(unit)),
        [c_at, at(-6, 0, 4)]
    );
    // D, 16 ticks on at a quarter meter, passed the dead body at 20 to reach 22.
    assert_eq!(
        [d, dead].map(|unit| walk.get::<Position>(unit)),
        [at(22, 0, 0), at(20, 0, 0)]
    );
    assert_eq!(
        [rooted, walker].map(|unit| walk.get::<Position>(unit)),
        [at(0, 0, 10), at(1, 0, 10)]
    );
    assert_eq!(walk.get::<Destination>(rooted).get(), Some(at(10, 0, 10)));
}

#[test]
fn a_client_parts_its_units_only_from_held_units_that_cannot_walk() {
    // Two own walkers along z = 0 and z = 4, a quarter meter a tick from x = 0 towards x = 4,
    // each with a held unit of the same size, 0.5 m, at x = 2 in its way: on z = 0 one that
    // cannot walk, like a tower; on z = 4 one that can, standing as the server last had it. In
    // tick 4, at 1, the first walker touches the first; in tick 5, at 1.25, it takes the whole
    // overlap and is back at 1. The second walker passes through the second, which may have
    // started walking since, to x = 4. Neither held unit moves.
    let quarter = Num::from_bits(1 << 22);
    let half = Num::from_bits(1 << 23);
    let mut walk = Walk::new();
    let blocked = walk.body(at(0, 0, 0), Some(at(4, 0, 0)), Some(quarter), half);
    let passing = walk.body(at(0, 0, 4), Some(at(4, 0, 4)), Some(quarter), half);
    let fixed = walk.body(at(2, 0, 0), None, None, half);
    let resting = walk.body(at(2, 0, 4), None, Some(quarter), half);
    // As on a client: the sim runs on no held unit, and collision names them.
    Unpredicted::register(&mut walk.world);
    for held in [fixed, resting] {
        let entity = walk.world.resource::<EntityIndex>().get(held).unwrap();
        walk.world.entity_mut(entity).insert(Unpredicted);
    }
    for _ in 0..16 {
        walk.tick();
    }
    let places = [blocked, passing, fixed, resting].map(|unit| walk.get::<Position>(unit));
    assert_eq!(places, [at(1, 0, 0), at(4, 0, 4), at(2, 0, 0), at(2, 0, 4)]);
}

#[test]
fn the_pathing_grid_follows_the_static_bodies_from_the_next_tick() {
    // 1 m cells over (−2, −2) to (2, 2), for walkers of 0.5 m: a tower of 0.5 m at (−1.5, −1.5)
    // blocks its own cell, 0, whose center is on it; one the client only holds, at (1.5, 1.5),
    // blocks cell 15. A walker never marks the grid.
    let half = Num::from_bits(1 << 23);
    let mut walk = Walk::new();
    let bounds = Bounds::new([num(-2), num(-2)], [num(2), num(2)]).unwrap();
    Navigation::load_pathing(
        &mut walk.world,
        Grid::new(Num::ONE, bounds).unwrap(),
        vec![half],
    );
    let quarter = |value: i64| Num::from_bits(value << 22);
    let place =
        |x: i64, z: i64| Position::new(Vec3::new(quarter(x), Num::ZERO, quarter(z))).unwrap();
    let blocked = |walk: &Walk| {
        let grid = walk.world.resource::<PathingGrid>();
        (0..16)
            .filter(|&cell| !grid.layer(half).open(cell))
            .collect::<Vec<_>>()
    };
    let tower = walk.body(place(-6, -6), None, None, half);
    let held = walk.body(place(6, 6), None, None, half);
    walk.body(place(0, 0), Some(at(0, 0, 1)), Some(Num::ONE), half);
    Unpredicted::register(&mut walk.world);
    let entity = walk.world.resource::<EntityIndex>().get(held).unwrap();
    walk.world.entity_mut(entity).insert(Unpredicted);
    assert_eq!(blocked(&walk), Vec::<usize>::new());
    walk.tick();
    assert_eq!(blocked(&walk), [0, 15]);
    // A tower that died stands no more in the way, from the next tick.
    let entity = walk.world.resource::<EntityIndex>().get(tower).unwrap();
    walk.world.entity_mut(entity).insert(Dead);
    walk.tick();
    assert_eq!(blocked(&walk), [15]);
}

#[test]
fn a_walker_goes_round_a_tower_and_never_touches_it() {
    // A tower of 0.9 m at the origin, on the line of a walker of 0.5 m from (−4, 0) to (4, 0), a
    // quarter meter a tick, over half-meter cells. Its route keeps 1.4 m off the tower's center at
    // every step, so collision never pushes it; with no pathing grid it walks into the tower, and
    // stops against it, pressed there.
    let half = Num::from_bits(1 << 23);
    let quarter = Num::from_bits(1 << 22);
    let tower_radius = Num::from_bits((9 << Num::FRAC_BITS) / 10);
    let reach = u128::from((tower_radius + half).to_bits().unsigned_abs());
    for planned in [true, false] {
        let mut walk = Walk::new();
        if planned {
            let bounds = Bounds::new([num(-8), num(-8)], [num(8), num(8)]).unwrap();
            Navigation::load_pathing(
                &mut walk.world,
                Grid::new(half, bounds).unwrap(),
                vec![half],
            );
        }
        walk.body(at(0, 0, 0), None, None, tower_radius);
        let walker = walk.body(at(-4, 0, 0), Some(at(4, 0, 0)), Some(quarter), half);
        let mut closest = u128::MAX;
        for _ in 0..80 {
            walk.tick();
            let offset = walk.get::<Position>(walker).get();
            closest = closest.min(offset.length_squared_bits());
        }
        let arrived = walk.get::<Destination>(walker).get().is_none();
        if planned {
            assert!(
                closest >= reach * reach,
                "{closest} against {}",
                reach * reach
            );
            assert!(arrived);
            assert_eq!(walk.get::<Position>(walker), at(4, 0, 0));
        } else {
            assert!(!arrived);
        }
    }
}

/// Whether the bodies of radii `a` and `b` at `first` and `second` overlap on the ground plane,
/// exactly.
fn overlap(first: Position, a: Num, second: Position, b: Num) -> bool {
    let reach = u128::from((a + b).to_bits().unsigned_abs());
    first.ground_offset(second).length_squared_bits() < reach * reach
}

#[test]
fn a_walker_goes_round_units_that_stand_in_its_way() {
    // A hero of 0.5 m stands at the origin and a creep of 0.35 m at (2, 0.5), on the line of a
    // walker of 0.5 m from (−4, 0) to (4, 0), a quarter meter a tick, over half-meter cells. It
    // goes round both, touching neither, and neither moves; with no pathing grid it walks into
    // the hero, yields to it, and stays pressed there.
    let half = Num::from_bits(1 << 23);
    let quarter = Num::from_bits(1 << 22);
    let creep_radius = Num::from_bits((35 << Num::FRAC_BITS) / 100);
    let creep_at = Position::new(Vec3::new(num(2), Num::ZERO, half)).unwrap();
    for planned in [true, false] {
        let mut walk = Walk::new();
        if planned {
            let bounds = Bounds::new([num(-8), num(-8)], [num(8), num(8)]).unwrap();
            let grid = Grid::new(half, bounds).unwrap();
            Navigation::load_pathing(&mut walk.world, grid, vec![creep_radius, half]);
        }
        let hero = walk.body(at(0, 0, 0), None, Some(quarter), half);
        let creep = walk.body(creep_at, None, Some(quarter), creep_radius);
        let walker = walk.body(at(-4, 0, 0), Some(at(4, 0, 0)), Some(quarter), half);
        let mut touched = false;
        for _ in 0..80 {
            walk.tick();
            let pos = walk.get::<Position>(walker);
            touched |= overlap(pos, half, at(0, 0, 0), half);
            touched |= overlap(pos, half, creep_at, creep_radius);
        }
        let arrived = walk.get::<Destination>(walker).get().is_none();
        let still = [walk.get::<Position>(hero), walk.get::<Position>(creep)];
        if planned {
            assert!(!touched);
            assert!(arrived);
            assert_eq!(walk.get::<Position>(walker), at(4, 0, 0));
            assert_eq!(still, [at(0, 0, 0), creep_at]);
        } else {
            assert!(!arrived);
        }
    }
}

#[test]
fn two_walkers_that_meet_head_on_pass_on_opposite_sides() {
    // Walkers of 0.5 m from (−4, 0) to (4, 0) and back, a quarter meter a tick: they meet, and
    // each, kept back by the other, goes round it on its own right, so they pass on opposite
    // sides of the line, the one bound for +x on −z, and both arrive. With no pathing grid they
    // push each other to a stop.
    let half = Num::from_bits(1 << 23);
    let quarter = Num::from_bits(1 << 22);
    for planned in [true, false] {
        let mut walk = Walk::new();
        if planned {
            let bounds = Bounds::new([num(-8), num(-8)], [num(8), num(8)]).unwrap();
            Navigation::load_pathing(
                &mut walk.world,
                Grid::new(half, bounds).unwrap(),
                vec![half],
            );
        }
        let east = walk.body(at(-4, 0, 0), Some(at(4, 0, 0)), Some(quarter), half);
        let west = walk.body(at(4, 0, 0), Some(at(-4, 0, 0)), Some(quarter), half);
        let mut sides = [Num::ZERO; 2];
        for _ in 0..120 {
            walk.tick();
            for (side, unit) in sides.iter_mut().zip([east, west]) {
                let z = walk.get::<Position>(unit).get().z;
                if z.to_bits().abs() > side.to_bits().abs() {
                    *side = z;
                }
            }
        }
        let arrived = [east, west].map(|unit| walk.get::<Destination>(unit).get().is_none());
        if planned {
            assert_eq!(arrived, [true, true]);
            assert_eq!(walk.get::<Position>(east), at(4, 0, 0));
            assert_eq!(walk.get::<Position>(west), at(-4, 0, 0));
            assert!(sides[0] < Num::ZERO && sides[1] > Num::ZERO, "{sides:?}");
        } else {
            assert_eq!(arrived, [false, false]);
        }
    }
}

#[test]
fn routes_wait_past_the_limit_of_expanded_cells_in_the_order_asked() {
    // A row of 16 cells of 1 m: a route along it expands each cell from the start to the goal
    // once, 16 to the far end, 4 to x = 3.5. A tick expands up to the grid's 16 cells.
    let mut walk = Walk::new();
    let bounds = Bounds::new([num(0), num(0)], [num(16), num(1)]).unwrap();
    Navigation::load_pathing(
        &mut walk.world,
        Grid::new(Num::ONE, bounds).unwrap(),
        vec![Num::ZERO],
    );
    let half = Num::from_bits(1 << 23);
    let place = |x: i64| Position::new(Vec3::new(num(x) + half, Num::ZERO, half)).unwrap();
    let (start, far, near) = (place(0), place(15), place(3));
    let units = [None, Some(far), Some(near), Some(far)].map(|goal| walk.unit(start, goal));
    let waiting = |walk: &Walk| {
        units.map(|unit| {
            let entity = walk.world.resource::<EntityIndex>().get(unit).unwrap();
            walk.world.get::<Route>(entity).unwrap().asked().is_some()
        })
    };
    // The last three ask in tick 0; the second expands 16 and meets the limit.
    walk.tick();
    assert_eq!(waiting(&walk), [false, false, true, true]);
    let second = walk.world.resource::<EntityIndex>().get(units[1]).unwrap();
    assert_eq!(walk.world.get::<Route>(second).unwrap().ahead(), [far]);
    // The first, of the lowest id, asks in tick 1, after the rest: they go first, 4 and 16, and
    // it waits.
    let first = walk.world.resource::<EntityIndex>().get(units[0]).unwrap();
    let mut destination = walk.world.get_mut::<Destination>(first).unwrap();
    destination.set(Some(near));
    walk.tick();
    assert_eq!(waiting(&walk), [true, false, false, false]);
    walk.tick();
    assert_eq!(waiting(&walk), [false; 4]);
}

#[test]
fn a_map_loads_only_if_the_widest_walker_reaches_every_waypoint_and_stands_on_every_spawn() {
    // A corridor 10 m by 4 m in half-meter cells, a lane along z = 2 from (1, 2) to (9, 2),
    // walkers of 0.5 m and towers of 0.9 m. Towers at (5, 0) and (5, 4) block the centers closer
    // than 1.4 m, z up to 1.25 and from 2.75 at x = 4.75 and 5.25, which leaves z = 1.75 and
    // 2.25 open: a gap a walker passes. One more at (5, 2) closes it.
    let tower_radius = Num::from_bits((9 << Num::FRAC_BITS) / 10);
    let half = Num::from_bits(1 << 23);
    let point = |x: i64, z: i64| GroundPoint([Scalar::Int(x), Scalar::Int(z)]);
    let tower = |x: i64, z: i64| StructureData {
        unit_type: "tower".to_owned(),
        team: "west".to_owned(),
        path: None,
        pos: point(x, z),
    };
    let map = |towers: &[(i64, i64)], neutral: (i64, i64)| MapData {
        bounds: Bounds::new([num(0), num(0)], [num(10), num(4)]).unwrap(),
        grid: None,
        navigation: Some(GridData {
            cell: Scalar::Decimal(half),
        }),
        paths: vec![PathData {
            name: "lane".to_owned(),
            points: vec![point(1, 2), point(9, 2)],
        }],
        spawns: [("west".to_owned(), point(1, 1))].into(),
        structures: towers.iter().map(|&(x, z)| tower(x, z)).collect(),
        neutral_spawns: vec![NeutralSpawnData {
            unit_type: "camp".to_owned(),
            pos: point(neutral.0, neutral.1),
        }],
    };
    let body_of = |unit_type: &str| (unit_type == "tower").then_some(tower_radius);
    let check = |towers: &[(i64, i64)], neutral| {
        Navigation::check_map(&map(towers, neutral), half, body_of)
    };
    assert_eq!(check(&[(5, 0), (5, 4)], (8, 3)), Ok(()));
    let unreachable = MapProblem::WaypointUnreachable {
        path: "lane".to_owned(),
        waypoint: 1,
    };
    assert_eq!(check(&[(5, 0), (5, 2), (5, 4)], (8, 3)), Err(unreachable));
    // A tower 1 m from the lane's end, (9, 2), or from the west spawn, (1, 1), or from the
    // neutral spawn, (8, 3): closer than 1.4 m.
    let blocked = MapProblem::WaypointBlocked {
        path: "lane".to_owned(),
        waypoint: 1,
    };
    assert_eq!(check(&[(9, 3)], (8, 1)), Err(blocked));
    let spawn = MapProblem::SpawnBlocked {
        team: "west".to_owned(),
    };
    assert_eq!(check(&[(2, 1)], (8, 3)), Err(spawn));
    assert_eq!(
        check(&[(7, 3)], (8, 3)),
        Err(MapProblem::NeutralSpawnBlocked { spawn: 0 })
    );
}

#[test]
fn a_dead_unit_forgets_its_destination_and_a_stopped_one_keeps_it() {
    let mut walk = Walk::new();
    let unit = walk.unit(at(0, 0, 0), Some(at(0, 0, 5)));
    let entity = walk.world.resource::<EntityIndex>().get(unit).unwrap();
    walk.world.entity_mut(entity).insert(Dead);
    // Each state that stops moving holds its unit where it stands, its destination kept, until
    // the state ends; one that does not, as disarmed, lets it walk.
    let states = [
        (UnitState::Stunned, false),
        (UnitState::Airborne, false),
        (UnitState::Rooted, false),
        (UnitState::Disarmed, true),
    ];
    let walkers = states.map(|(state, _)| {
        let x = 2 * (state as i64 + 1);
        let id = walk.unit(at(x, 0, 0), Some(at(x, 0, 5)));
        walk.set_states(id, &[state]);
        (id, x)
    });
    walk.tick();
    assert_eq!(walk.get::<Position>(unit), at(0, 0, 0));
    assert_eq!(walk.get::<Destination>(unit).get(), None);
    for (&(id, x), (state, walks)) in walkers.iter().zip(states) {
        let z = i64::from(walks);
        assert_eq!(walk.get::<Position>(id), at(x, 0, z), "{state:?}");
        assert_eq!(
            walk.get::<Destination>(id).get(),
            Some(at(x, 0, 5)),
            "{state:?}"
        );
        walk.set_states(id, &[]);
    }
    // The states end: each walks on a meter from where it stood.
    walk.tick();
    for (&(id, x), (state, walks)) in walkers.iter().zip(states) {
        let z = i64::from(walks) + 1;
        assert_eq!(walk.get::<Position>(id), at(x, 0, z), "{state:?}");
    }
}

#[test]
fn paths_count_waypoints_in_either_direction() {
    let paths = Paths::new([
        ("near", &[at(0, 0, 0), at(1, 0, 0)][..]),
        ("far", &[at(5, 0, 5)][..]),
    ]);
    assert_eq!(paths.count(), 2);
    assert_eq!(
        (paths.named("far"), paths.named("none")),
        (Some(PathId::new(1)), None)
    );
    assert_eq!(
        (paths.name(PathId::new(0)), paths.name(PathId::new(1))),
        ("near", "far")
    );
    let walk = |path, direction| {
        (0..3)
            .map(|index| paths.waypoint(path, index, direction))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        walk(PathId::new(0), PathDirection::Forward),
        [Some(at(0, 0, 0)), Some(at(1, 0, 0)), None]
    );
    assert_eq!(
        walk(PathId::new(0), PathDirection::Backward),
        [Some(at(1, 0, 0)), Some(at(0, 0, 0)), None]
    );
    assert_eq!(
        walk(PathId::new(1), PathDirection::Backward),
        [Some(at(5, 0, 5)), None, None]
    );
    assert_eq!(
        walk(PathId::new(2), PathDirection::Forward),
        [None, None, None]
    );
}

#[test]
fn every_navigation_type_is_state() {
    let mut walk = Walk::new();
    let unit = walk.unit(at(0, 0, 0), Some(at(0, 0, 5)));
    let entity = walk.world.resource::<EntityIndex>().get(unit).unwrap();
    walk.world.entity_mut(entity).insert((
        PathWalker::start(PathDirection::Forward),
        OnPath::new(PathId::new(0)),
    ));
    let mut route = walk.world.get_mut::<Route>(entity).unwrap();
    route.ask(at(3, 0, 4), Tick::new(2));
    let registry = &walk.registry;
    let mut per_type = Vec::new();
    let hash = registry.hash_by_type(&walk.world, &mut per_type);
    let names: Vec<_> = per_type.iter().map(|TypeHash { name, .. }| *name).collect();
    assert_eq!(
        names,
        [
            "navigation.destination",
            "navigation.move_step",
            "navigation.on_path",
            "navigation.path_walker",
            "navigation.progress",
            "navigation.route",
            "sim.entities",
            "sim.id_allocator",
            "sim.position",
            "sim.tick",
            "units.body",
            "units.owner",
            "units.spawn_point",
            "units.team",
            "units.unit_type",
        ]
    );
    let mut snapshot = Vec::new();
    registry.snapshot(&walk.world, &mut snapshot);
    let mut restored = Walk::new();
    registry.restore(&snapshot, &mut restored.world).unwrap();
    assert_eq!(registry.hash(&restored.world), hash);
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
