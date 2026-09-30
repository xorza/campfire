use std::num::NonZeroU32;

use bevy_ecs::component::Component;
use campfire_math::Vec3;
use campfire_sim::{Capability, EntityIndex, IdAllocator, SimUpdate, Tick, TickRate, TypeHash};

use super::*;
use crate::capability_set::internals::TestMatch;
use crate::navigation::path_walker::PathDirection;
use crate::units::path_id::PathId;

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
    let (far, near) = (place(15), place(3));
    let units = [far, near, near, far].map(|_| walk.unit(place(0), None));
    let ask = |walk: &mut Walk, unit: StableId, goal: Position, tick: u64| {
        let entity = walk.world.resource::<EntityIndex>().get(unit).unwrap();
        let mut route = walk.world.get_mut::<Route>(entity).unwrap();
        route.ask(goal, Tick::new(tick));
    };
    let waiting = |walk: &Walk| {
        units.map(|unit| {
            let entity = walk.world.resource::<EntityIndex>().get(unit).unwrap();
            walk.world.get::<Route>(entity).unwrap().asked().is_some()
        })
    };
    for (unit, goal) in units.into_iter().zip([far, near, near, far]) {
        ask(&mut walk, unit, goal, 0);
    }
    // The first expands 16 and meets the limit.
    walk.tick();
    assert_eq!(waiting(&walk), [false, true, true, true]);
    let entity = walk.world.resource::<EntityIndex>().get(units[0]).unwrap();
    assert_eq!(walk.world.get::<Route>(entity).unwrap().waypoints(), [far]);
    // The first asks again, after the rest: they go first, 4, 4 and 16, and it waits.
    ask(&mut walk, units[0], far, 1);
    walk.tick();
    assert_eq!(waiting(&walk), [true, false, false, false]);
    walk.tick();
    assert_eq!(waiting(&walk), [false; 4]);
}

#[test]
fn a_dead_unit_stays_and_forgets_its_destination() {
    let mut walk = Walk::new();
    let unit = walk.unit(at(0, 0, 0), Some(at(0, 0, 5)));
    let entity = walk.world.resource::<EntityIndex>().get(unit).unwrap();
    walk.world.entity_mut(entity).insert(Dead);
    walk.tick();
    assert_eq!(walk.get::<Position>(unit), at(0, 0, 0));
    assert_eq!(walk.get::<Destination>(unit).get(), None);
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
