use bevy_ecs::change_detection::DetectChanges;
use campfire_common::Tick;

use super::*;
use crate::capability_set::test_match::TestMatch;
use crate::mode::error::ModeError;
use crate::mode::map_data::{
    MapData, MapNavigationData, MapPoint, MarkerData, PathData, PlacedUnitData, WallData,
};
use crate::navigation::error::MapProblem;
use crate::navigation::navigation_rules::NavigationRules;
use crate::navigation::path_walker::PathEnd;
use crate::navigation::wall::Wall;
use crate::units::layer::Layer;
use crate::units::path_id::PathId;
use crate::values::declared_name::DeclaredName;
use crate::values::polygon::Polygon;
use crate::values::polygon::error::PolygonError;
use crate::values::scalar::Scalar;

const ONE: i64 = 1 << 24;
fn at(x: i64, y: i64, z: i64) -> Position {
    Position::new(Vec3::new(Num::int(x), Num::int(y), Num::int(z))).unwrap()
}

/// The second layer, as an RTS's air.
const AIR: Layer = Layer::new(1);

/// A walker of `radius` on the first layer.
const fn ground(radius: Num) -> Walker {
    Walker {
        layer: Layer::FIRST,
        radius,
    }
}

#[derive(Debug)]
struct Walk {
    sim: TestMatch,
}

impl Walk {
    fn new() -> Walk {
        let sim = TestMatch::client(&[Capability::Navigation]);
        Walk { sim }
    }

    /// Loads a pathing grid of `cell` cells from `min` to `max` for `walkers`.
    fn load_pathing(&mut self, cell: Num, min: [i64; 2], max: [i64; 2], walkers: Vec<Walker>) {
        self.load_walled(cell, min, max, walkers, &[]);
    }

    /// Loads a pathing grid as `load_pathing` does, with `walls`.
    fn load_walled(
        &mut self,
        cell: Num,
        min: [i64; 2],
        max: [i64; 2],
        walkers: Vec<Walker>,
        walls: &[Wall],
    ) {
        let bounds = Bounds::new(min.map(Num::int), max.map(Num::int)).unwrap();
        let grid = Grid::new(cell, bounds).unwrap();
        let terrain = Terrain::new(&grid, walls);
        Navigation::load_pathing(&mut self.sim.world, grid, &terrain, walkers);
    }

    /// A unit at `at` walking a meter a tick to `to`.
    fn unit(&mut self, at: Position, to: Option<Position>) -> StableId {
        let id = self
            .sim
            .spawn(at, Navigation::walker(MoveStep::new(Num::ONE).unwrap()));
        self.sim.get_mut::<Destination>(id).set(to);
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
        self.body_on(at, to, step, Body::new(radius).unwrap())
    }

    /// A unit at `at` with `body`, walking `step` a tick to `to`, or one that does not walk when
    /// `step` is `None`.
    fn body_on(
        &mut self,
        at: Position,
        to: Option<Position>,
        step: Option<Num>,
        body: Body,
    ) -> StableId {
        let id = self.sim.spawn(at, body);
        if let Some(step) = step {
            self.sim
                .insert(id, Navigation::walker(MoveStep::new(step).unwrap()));
            self.sim.get_mut::<Destination>(id).set(to);
        }
        id
    }

    fn get_route(&self, id: StableId) -> &Route {
        let entity = self.sim.entity(id);
        self.sim.world.get::<Route>(entity).unwrap()
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

    walk.sim.step();
    assert_eq!(*walk.sim.get::<Position>(straight), at(0, 0, 1));
    assert_eq!(
        *walk.sim.get::<Position>(slanted),
        Position::new(Vec3::new(
            Num::from_bits(4 * ONE + 10_066_330),
            Num::int(2),
            Num::from_bits(13_421_773)
        ))
        .unwrap()
    );
    assert_eq!(*walk.sim.get::<Position>(still), at(9, 0, 9));
    // Straight along z, one meter a tick: done in the fifth tick, and the destination dropped.
    for z in 2..=5 {
        walk.sim.step();
        assert_eq!(*walk.sim.get::<Position>(straight), at(0, 0, z));
    }
    assert_eq!(walk.sim.get::<Destination>(straight).get(), None);
    walk.sim.step();
    assert_eq!(*walk.sim.get::<Position>(straight), at(0, 0, 5));
}

#[test]
fn every_unit_that_walks_ends_the_tick_within_the_bounds() {
    let mut walk = Walk::new();
    assert_eq!(*walk.sim.world.resource::<Bounds>(), Bounds::WORLD);
    walk.sim.world.insert_resource(
        Bounds::new([Num::int(-4), Num::int(-4)], [Num::int(4), Num::int(4)]).unwrap(),
    );
    // Outside the bounds at x = 6: clamped to the edge x = 4, at its height and its z.
    let outside = walk.unit(at(6, 1, 2), None);
    // From x = 3 a meter towards x = 5: it arrives at 4, then walks on to 5 and back to 4.
    let past = walk.unit(at(3, 0, 0), Some(at(5, 0, 0)));
    let inside = walk.unit(at(1, 0, 1), None);
    let id = walk.sim.spawn(at(9, 0, 9), ());

    walk.sim.step();
    assert_eq!(*walk.sim.get::<Position>(outside), at(4, 1, 2));
    assert_eq!(*walk.sim.get::<Position>(past), at(4, 0, 0));
    assert_eq!(*walk.sim.get::<Position>(inside), at(1, 0, 1));
    walk.sim.step();
    assert_eq!(*walk.sim.get::<Position>(past), at(4, 0, 0));
    assert_eq!(walk.sim.get::<Destination>(past).get(), None);
    // A unit that does not walk is not moved: the map's check keeps it within the bounds.
    assert_eq!(*walk.sim.get::<Position>(id), at(9, 0, 9));
}

#[test]
fn bodies_part_and_block_the_way() {
    // Bodies of 0.5 m walking a quarter meter a tick. A from x = 0 and B from x = 3 walk at each
    // other: 3 − 0.5 t apart after tick t, and in tick 5, at 1.25 and 1.75, they overlap by half
    // a meter, which parts them a quarter meter each, to 1 and 2: 1 m apart, the sum of their
    // radii. Every tick after, they walk in and part again, to the same places.
    let quarter = Num::QUARTER;
    let half = Num::HALF;
    let mut walk = Walk::new();
    let a = walk.body(at(0, 0, 0), Some(at(10, 0, 0)), Some(quarter), half);
    let b = walk.body(at(3, 0, 0), Some(at(-10, 0, 0)), Some(quarter), half);
    // A tower of radius 1 at (−6, 4), which does not walk, and C walking at it along z from 0: C
    // touches it at z = 2.5, 1.5 m off, and in tick 11, at 2.75, is pushed back all the way.
    let tower = walk.body(at(-6, 0, 4), None, None, Num::ONE);
    let c = walk.body(at(-6, 0, 0), Some(at(-6, 0, 8)), Some(quarter), half);
    // D walks through a dead body on its way.
    let dead = walk.body(at(20, 0, 0), None, Some(quarter), half);
    walk.sim.insert(dead, Dead);
    let d = walk.body(at(18, 0, 0), Some(at(22, 0, 0)), Some(quarter), half);
    // A walker at a rooted unit on its way along z = 10, which so stands: the walker touches it
    // in tick 8, at x = 1, and from tick 9 takes the whole overlap, back to 1 each tick, as a
    // walker takes it from a unit that stands. The rooted unit keeps its place and destination.
    let rooted = walk.body(at(0, 0, 10), Some(at(10, 0, 10)), Some(quarter), half);
    walk.sim.set_blocks(rooted, &[Block::Move]);
    let walker = walk.body(at(3, 0, 10), Some(at(-10, 0, 10)), Some(quarter), half);
    for _ in 0..5 {
        walk.sim.step();
    }
    assert_eq!(
        [a, b].map(|unit| *walk.sim.get::<Position>(unit)),
        [at(1, 0, 0), at(2, 0, 0)]
    );
    for _ in 5..16 {
        walk.sim.step();
    }
    assert_eq!(
        [a, b].map(|unit| *walk.sim.get::<Position>(unit)),
        [at(1, 0, 0), at(2, 0, 0)]
    );
    let c_at = Position::new(Vec3::new(Num::int(-6), Num::ZERO, Num::int(2) + half)).unwrap();
    assert_eq!(
        [c, tower].map(|unit| *walk.sim.get::<Position>(unit)),
        [c_at, at(-6, 0, 4)]
    );
    // D, 16 ticks on at a quarter meter, passed the dead body at 20 to reach 22.
    assert_eq!(
        [d, dead].map(|unit| *walk.sim.get::<Position>(unit)),
        [at(22, 0, 0), at(20, 0, 0)]
    );
    assert_eq!(
        [rooted, walker].map(|unit| *walk.sim.get::<Position>(unit)),
        [at(0, 0, 10), at(1, 0, 10)]
    );
    assert_eq!(
        walk.sim.get::<Destination>(rooted).get(),
        Some(at(10, 0, 10))
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
    let quarter = Num::QUARTER;
    let half = Num::HALF;
    let mut walk = Walk::new();
    let blocked = walk.body(at(0, 0, 0), Some(at(4, 0, 0)), Some(quarter), half);
    let passing = walk.body(at(0, 0, 4), Some(at(4, 0, 4)), Some(quarter), half);
    let fixed = walk.body(at(2, 0, 0), None, None, half);
    let resting = walk.body(at(2, 0, 4), None, Some(quarter), half);
    // As on a client: the sim runs on no held unit, and collision names them.
    Unpredicted::register(&mut walk.sim.world);
    for held in [fixed, resting] {
        walk.sim.insert(held, Unpredicted);
    }
    for _ in 0..16 {
        walk.sim.step();
    }
    let places = [blocked, passing, fixed, resting].map(|unit| *walk.sim.get::<Position>(unit));
    assert_eq!(places, [at(1, 0, 0), at(4, 0, 4), at(2, 0, 0), at(2, 0, 4)]);
}

#[test]
fn the_pathing_grid_follows_the_static_bodies_from_the_next_tick() {
    // 1 m cells over (−2, −2) to (2, 2), for walkers of 0.5 m: a tower of 0.5 m at (−1.5, −1.5)
    // blocks its own cell, 0, whose center is on it; one the client only holds, at (1.5, 1.5),
    // blocks cell 15. A walker never marks the grid.
    let half = Num::HALF;
    let mut walk = Walk::new();
    walk.load_pathing(Num::ONE, [-2, -2], [2, 2], vec![ground(half)]);
    let quarter = |value: i64| Num::from_bits(value << 22);
    let place =
        |x: i64, z: i64| Position::new(Vec3::new(quarter(x), Num::ZERO, quarter(z))).unwrap();
    let blocked = |walk: &Walk| {
        let grid = walk.sim.world.resource::<PathingGrid>();
        (0..16)
            .filter(|&cell| !grid.clearance(ground(half)).open(cell))
            .collect::<Vec<_>>()
    };
    let tower = walk.body(place(-6, -6), None, None, half);
    let held = walk.body(place(6, 6), None, None, half);
    walk.body(place(0, 0), Some(at(0, 0, 1)), Some(Num::ONE), half);
    Unpredicted::register(&mut walk.sim.world);
    walk.sim.insert(held, Unpredicted);
    assert_eq!(blocked(&walk), Vec::<usize>::new());
    walk.sim.step();
    assert_eq!(blocked(&walk), [0, 15]);
    // A tower that died stands no more in the way, from the next tick.
    walk.sim.insert(tower, Dead);
    walk.sim.step();
    assert_eq!(blocked(&walk), [15]);
}

#[test]
fn a_walker_goes_round_a_tower_and_never_touches_it() {
    // A tower of 0.9 m at the origin, on the line of a walker of 0.5 m from (−4, 0) to (4, 0), a
    // quarter meter a tick, over half-meter cells. Its route keeps 1.4 m off the tower's center at
    // every step, so collision never pushes it; with no pathing grid it walks into the tower, and
    // stops against it, pressed there.
    let half = Num::HALF;
    let quarter = Num::QUARTER;
    let tower_radius = Num::from_bits((9 << Num::FRAC_BITS) / 10);
    let reach = u128::from((tower_radius + half).to_bits().unsigned_abs());
    for planned in [true, false] {
        let mut walk = Walk::new();
        if planned {
            walk.load_pathing(half, [-8, -8], [8, 8], vec![ground(half)]);
        }
        walk.body(at(0, 0, 0), None, None, tower_radius);
        let walker = walk.body(at(-4, 0, 0), Some(at(4, 0, 0)), Some(quarter), half);
        let mut closest = u128::MAX;
        let mut arrived = None;
        for tick in 0..80 {
            walk.sim.step();
            let offset = walk.sim.get::<Position>(walker).get();
            closest = closest.min(offset.length_squared_bits());
            if arrived.is_none() && walk.sim.get::<Destination>(walker).get().is_none() {
                arrived = Some(tick);
            }
        }
        if planned {
            assert!(
                closest >= reach * reach,
                "{closest} against {}",
                reach * reach
            );
            // Three ticks more than the 32 a straight 8 m takes, a quarter meter a tick.
            assert_eq!(arrived, Some(34));
            assert_eq!(*walk.sim.get::<Position>(walker), at(4, 0, 0));
        } else {
            // It stops against the tower, the two radii from its centre, pressed there.
            assert_eq!(arrived, None);
            let stop = Position::new(Vec3::new(-(tower_radius + half), Num::ZERO, Num::ZERO));
            assert_eq!(*walk.sim.get::<Position>(walker), stop.unwrap());
        }
    }
}

#[test]
fn a_walker_goes_round_a_wall_smoothed_past_its_corners_and_never_through_it() {
    // Over 1 m cells from (0, 0) to (10, 6), a wall from (4, 0) to (6, 4) blocks the cells whose
    // centers it holds, columns 4 and 5 of rows 0 to 3, to a walker of no body. One at (2, 1)
    // walks half a meter a tick to (8, 1).
    let half = Num::HALF;
    let mut walk = Walk::new();
    let box_of = |[x0, z0]: [i64; 2], [x1, z1]: [i64; 2]| {
        let area = [[x0, z0], [x1, z0], [x1, z1], [x0, z1]].map(|point| point.map(Num::int));
        Polygon::new(area.to_vec()).unwrap()
    };
    let barrier = Wall {
        layer: Layer::FIRST,
        area: box_of([4, 0], [6, 4]),
    };
    walk.load_walled(
        Num::ONE,
        [0, 0],
        [10, 6],
        vec![ground(Num::ZERO)],
        &[barrier],
    );
    let walker = walk.unit(at(2, 0, 1), Some(at(8, 0, 1)));
    // Its route goes over the wall's top, row 4, and keeps a waypoint only where the line on would
    // touch a blocked cell: from (2, 1) to the next cell, (4.5, 4.5), it is at z = 3.8 at x = 4,
    // on cell (4, 3); from (3.5, 4.5) to the cell after (6.5, 4.5), (7.5, 3.5), at z = 3.875 at
    // x = 6, on cell (5, 3). From (6.5, 4.5) it goes straight to its goal.
    let point = |x: i64, z: i64| Position::new(Vec3::new(half * x, Num::ZERO, half * z)).unwrap();
    walk.sim.step();
    let corners = [point(7, 9), point(13, 9), point(16, 2)];
    assert_eq!(walk.get_route(walker).ahead(), corners);
    // It walks √14.5 ≈ 3.81 m, 3 m and √14.5 m, a meter a tick, never into the wall's square,
    // and arrives in its eleventh tick.
    let mut steps = 1;
    while walk.sim.get::<Destination>(walker).get().is_some() {
        let pos = walk.sim.get::<Position>(walker).get();
        let inside =
            |value: Num, low: i64, high: i64| Num::int(low) <= value && value <= Num::int(high);
        assert!(!(inside(pos.x, 4, 6) && inside(pos.z, 0, 4)), "{pos:?}");
        walk.sim.step();
        steps += 1;
    }
    assert_eq!(steps, 11);
    assert_eq!(*walk.sim.get::<Position>(walker), at(8, 0, 1));
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
    let half = Num::HALF;
    let quarter = Num::QUARTER;
    let creep_radius = Num::from_bits((35 << Num::FRAC_BITS) / 100);
    let creep_at = Position::new(Vec3::new(Num::int(2), Num::ZERO, half)).unwrap();
    for planned in [true, false] {
        let mut walk = Walk::new();
        if planned {
            walk.load_pathing(
                half,
                [-8, -8],
                [8, 8],
                vec![ground(creep_radius), ground(half)],
            );
        }
        let hero = walk.body(at(0, 0, 0), None, Some(quarter), half);
        let creep = walk.body(creep_at, None, Some(quarter), creep_radius);
        let walker = walk.body(at(-4, 0, 0), Some(at(4, 0, 0)), Some(quarter), half);
        let mut touched = false;
        let mut arrived = None;
        for tick in 0..80 {
            walk.sim.step();
            let pos = *walk.sim.get::<Position>(walker);
            touched |= overlap(pos, half, at(0, 0, 0), half);
            touched |= overlap(pos, half, creep_at, creep_radius);
            if arrived.is_none() && walk.sim.get::<Destination>(walker).get().is_none() {
                arrived = Some(tick);
            }
        }
        let still = [
            *walk.sim.get::<Position>(hero),
            *walk.sim.get::<Position>(creep),
        ];
        if planned {
            assert!(!touched);
            // Seven ticks more than the 32 a straight 8 m takes, round both.
            assert_eq!(arrived, Some(38));
            assert_eq!(*walk.sim.get::<Position>(walker), at(4, 0, 0));
            assert_eq!(still, [at(0, 0, 0), creep_at]);
        } else {
            // Against the hero, the two radii, 1 m, from its centre.
            assert_eq!(arrived, None);
            assert_eq!(*walk.sim.get::<Position>(walker), at(-1, 0, 0));
        }
    }
}

#[test]
fn two_walkers_that_meet_head_on_pass_on_opposite_sides() {
    // Walkers of 0.5 m from (−4, 0) to (4, 0) and back, a quarter meter a tick: they meet, and
    // each, kept back by the other, goes round it on its own right, so they pass on opposite
    // sides of the line, the one bound for +x on −z, and both arrive. With no pathing grid they
    // push each other to a stop.
    let half = Num::HALF;
    let quarter = Num::QUARTER;
    for planned in [true, false] {
        let mut walk = Walk::new();
        if planned {
            walk.load_pathing(half, [-8, -8], [8, 8], vec![ground(half)]);
        }
        let east = walk.body(at(-4, 0, 0), Some(at(4, 0, 0)), Some(quarter), half);
        let west = walk.body(at(4, 0, 0), Some(at(-4, 0, 0)), Some(quarter), half);
        let mut sides = [Num::ZERO; 2];
        let mut arrived = [None; 2];
        for tick in 0..120 {
            walk.sim.step();
            for (side, unit) in sides.iter_mut().zip([east, west]) {
                let z = walk.sim.get::<Position>(unit).get().z;
                if z.to_bits().abs() > side.to_bits().abs() {
                    *side = z;
                }
            }
            for (at, unit) in arrived.iter_mut().zip([east, west]) {
                if at.is_none() && walk.sim.get::<Destination>(unit).get().is_none() {
                    *at = Some(tick);
                }
            }
        }
        if planned {
            // Seven ticks more each than the 32 a straight 8 m takes, round the other.
            assert_eq!(arrived, [Some(38); 2]);
            assert_eq!(*walk.sim.get::<Position>(east), at(4, 0, 0));
            assert_eq!(*walk.sim.get::<Position>(west), at(-4, 0, 0));
            assert!(sides[0] < Num::ZERO && sides[1] > Num::ZERO, "{sides:?}");
        } else {
            // Pressed together, each half a metre from where they met, at x = 0.
            assert_eq!(arrived, [None; 2]);
            let half_x = |x: Num| Position::new(Vec3::new(x, Num::ZERO, Num::ZERO)).unwrap();
            let pressed = [east, west].map(|unit| *walk.sim.get::<Position>(unit));
            assert_eq!(pressed, [half_x(-half), half_x(half)]);
        }
    }
}

#[test]
fn an_air_unit_passes_over_a_ground_unit_and_a_wall() {
    // Over 1 m cells from (0, 0) to (12, 6), a wall of three towers of 1 m on the ground at
    // x = 6, z = 1, 3 and 5, touching, closes the map to walkers of 0.5 m: every cell center of
    // columns 5 and 6 is 0.71 m from a tower, closer than 1.5. A ground unit of 0.5 m stands at
    // (3, 3).
    let half = Num::HALF;
    let mut walk = Walk::new();
    let flyer = Walker {
        layer: AIR,
        radius: half,
    };
    walk.load_pathing(Num::ONE, [0, 0], [12, 6], vec![ground(half), flyer]);
    for z in [1, 3, 5] {
        walk.body(at(6, 0, z), None, None, Num::ONE);
    }
    let standing = walk.body(at(3, 0, 3), None, Some(Num::ONE), half);
    // An air unit of 0.5 m from (1, 3) to (11, 3), a meter a tick, flies straight over both: in
    // tick 2 it is on the ground unit, which does not move, and in tick 10 at its goal.
    let body = Body::new(half).unwrap().on(AIR);
    let flying = walk.body_on(at(1, 0, 3), Some(at(11, 0, 3)), Some(Num::ONE), body);
    // A ground unit of 0.5 m from (1, 1) to (11, 1) stops short of the wall, at the cell center
    // nearest its goal that it reaches: (4.5, 0.5) and (4.5, 1.5) tie at √42.5 ≈ 6.52 m, each
    // √2.5 ≈ 1.58 m from the tower at (6, 1), and the lower cell number wins. It walks straight
    // there, √12.5 ≈ 3.54 m, so it arrives in tick 4.
    let walking = walk.body(at(1, 0, 1), Some(at(11, 0, 1)), Some(Num::ONE), half);
    let short = Position::new(Vec3::new(Num::int(4) + half, Num::ZERO, half)).unwrap();
    let mut flown = Vec::new();
    for _ in 0..12 {
        walk.sim.step();
        flown.push(*walk.sim.get::<Position>(flying));
        assert_eq!(*walk.sim.get::<Position>(standing), at(3, 0, 3));
    }
    let line: Vec<Position> = (2..=11).chain([11; 2]).map(|x| at(x, 0, 3)).collect();
    assert_eq!(flown, line);
    assert_eq!(*walk.sim.get::<Position>(walking), short);
}

#[test]
fn routes_wait_past_the_limit_of_work_in_the_order_asked() {
    // A row of 16 cells of 1 m: a route along it tests its goal, expands each cell from the start
    // to the goal once, and tests the line from the start to each cell but the two nearest it:
    // 1 + 16 + 14 = 31 to the far end, 1 + 4 + 2 = 7 to x = 3.5, 1 + 15 + 13 = 29 to x = 14.5.
    // A tick does up to the grid's 16.
    let mut walk = Walk::new();
    walk.load_pathing(Num::ONE, [0, 0], [16, 1], vec![ground(Num::ZERO)]);
    let half = Num::HALF;
    let place = |x: i64| Position::new(Vec3::new(Num::int(x) + half, Num::ZERO, half)).unwrap();
    let (start, far, near) = (place(0), place(15), place(3));
    let units = [None, Some(far), Some(near), Some(far)].map(|goal| walk.unit(start, goal));
    let waiting = |walk: &Walk| {
        units.map(|unit| {
            let entity = walk.sim.entity(unit);
            walk.sim
                .world
                .get::<Route>(entity)
                .unwrap()
                .asked()
                .is_some()
        })
    };
    // The last three ask in tick 0; the second does 31 and meets the limit.
    walk.sim.step();
    assert_eq!(waiting(&walk), [false, false, true, true]);
    let second = walk.sim.entity(units[1]);
    assert_eq!(walk.sim.world.get::<Route>(second).unwrap().ahead(), [far]);
    // The first, of the lowest id, asks in tick 1, after the rest. The last asks again in tick
    // 1, for x = 14.5, and keeps its place of tick 0. So the third and the last go first, 7 and
    // 29, which meets the limit, and the first waits; had the last's ask moved to tick 1, the
    // first and the third would go before it, 7 each, and none would wait.
    let first = walk.sim.entity(units[0]);
    let mut destination = walk.sim.world.get_mut::<Destination>(first).unwrap();
    destination.set(Some(near));
    let last = walk.sim.entity(units[3]);
    let mut destination = walk.sim.world.get_mut::<Destination>(last).unwrap();
    destination.set(Some(place(14)));
    walk.sim.step();
    assert_eq!(waiting(&walk), [true, false, false, false]);
    walk.sim.step();
    assert_eq!(waiting(&walk), [false; 4]);
}

#[test]
fn a_walker_steers_with_the_work_the_routes_left() {
    // A walker of 0.25 m from (0.5, 1.5) to (11.5, 1.5) on 12 by 3 cells of 1 m, a limit of 36:
    // its route in tick 0 is the straight line, 1 + 12 + 10 = 23. After it, a unit of 0.25 m
    // stands on the line at (3.5, 1.5), and walkers with no body ask for routes along z = 0.5,
    // 23 each. With none or one, work is left in tick 1, and the walker steers round the unit;
    // with two, they do 46, and the walker keeps its route, and steers in tick 2.
    let quarter = Num::QUARTER;
    let half = Num::HALF;
    let place = |x: i64, z: i64| {
        Position::new(Vec3::new(Num::int(x) + half, Num::ZERO, Num::int(z) + half))
    };
    let goal = place(11, 1).unwrap();
    for (askers, steered) in [(0, [true, true]), (1, [true, true]), (2, [false, true])] {
        let mut walk = Walk::new();
        walk.load_pathing(
            Num::ONE,
            [0, 0],
            [12, 3],
            vec![ground(Num::ZERO), ground(quarter)],
        );
        let walker = walk.body(place(0, 1).unwrap(), Some(goal), Some(quarter), quarter);
        walk.sim.step();
        assert_eq!(walk.get_route(walker).ahead(), [goal]);
        walk.body(place(3, 1).unwrap(), None, Some(quarter), quarter);
        for _ in 0..askers {
            walk.unit(place(0, 0).unwrap(), Some(place(11, 0).unwrap()));
        }
        let ticks = [(); 2].map(|()| {
            walk.sim.step();
            walk.get_route(walker).ahead() != [goal]
        });
        assert_eq!(ticks, steered, "{askers}");
    }
}

#[test]
fn a_walker_asks_again_only_for_a_static_body_put_in_its_way() {
    // A walker of 0.25 m from (0.5, 1.5) to (7.5, 1.5) on 8 by 3 cells of 1 m, a meter a tick:
    // its route is the straight line. A tower of 0.5 m at (5.5, 0.5) comes 1 m from the line,
    // past the two radii, 0.75 m: the route stays as it was. One at (5.5, 1.5) stands on it: the
    // walker asks again in the next tick, and goes round it.
    let quarter = Num::QUARTER;
    let half = Num::HALF;
    let place = |x: i64, z: i64| {
        Position::new(Vec3::new(Num::int(x) + half, Num::ZERO, Num::int(z) + half))
    };
    let mut walk = Walk::new();
    walk.load_pathing(Num::ONE, [0, 0], [8, 3], vec![ground(quarter)]);
    let goal = place(7, 1).unwrap();
    let walker = walk.body(place(0, 1).unwrap(), Some(goal), Some(Num::ONE), quarter);
    walk.sim.step();
    let entity = walk.sim.entity(walker);
    let changed = |walk: &Walk| {
        walk.sim
            .world
            .entity(entity)
            .get_ref::<Route>()
            .unwrap()
            .last_changed()
    };
    let planned = changed(&walk);
    walk.body(place(5, 0).unwrap(), None, None, half);
    walk.sim.step();
    assert_eq!(changed(&walk), planned);
    assert_eq!(walk.get_route(walker).ahead(), [goal]);
    walk.body(place(5, 1).unwrap(), None, None, half);
    walk.sim.step();
    assert_ne!(walk.get_route(walker).ahead(), [goal]);
    assert!(walk.get_route(walker).reached());
}

#[test]
fn a_walker_that_arrives_short_waits_there_until_a_static_body_goes() {
    // Towers of 0.5 m down column 4 of 8 by 3 cells of 1 m wall off a walker of 0.25 m at (0.5,
    // 1.5) from its goal, (6.5, 1.5): each blocks its own cell alone, whose center is on it, and
    // not its neighbors', 1 m off, past the two radii, 0.75 m. Its route ends at the nearest
    // cell it reaches, (3.5, 1.5), 3 m on, which it arrives at in tick 2, a meter a tick.
    let quarter = Num::QUARTER;
    let half = Num::HALF;
    let place = |x: i64, z: i64| {
        Position::new(Vec3::new(Num::int(x) + half, Num::ZERO, Num::int(z) + half))
    };
    let goal = place(6, 1).unwrap();
    let walled = || {
        let mut walk = Walk::new();
        walk.load_pathing(Num::ONE, [0, 0], [8, 3], vec![ground(quarter)]);
        let towers = [0, 1, 2].map(|z| walk.body(place(4, z).unwrap(), None, None, half));
        let walker = walk.body(place(0, 1).unwrap(), Some(goal), Some(Num::ONE), quarter);
        let middle = walk.sim.entity(towers[1]);
        (walk, middle, walker)
    };
    // The middle tower dies while the walker is on its way, at (1.5, 1.5) after tick 0: its route
    // ends short, so it asks again in tick 1, and walks on straight to the goal, 6 m in 6 ticks.
    let (mut walk, middle, walker) = walled();
    walk.sim.step();
    walk.sim.world.entity_mut(middle).insert(Dead);
    walk.sim.step();
    assert!(walk.get_route(walker).reached());
    for _ in 0..4 {
        walk.sim.step();
    }
    assert_eq!(*walk.sim.get::<Position>(walker), goal);
    // The middle tower stands, and the walker arrives short.
    let (mut walk, middle, walker) = walled();
    for _ in 0..3 {
        walk.sim.step();
    }
    let short = place(3, 1).unwrap();
    assert_eq!(*walk.sim.get::<Position>(walker), short);
    assert_eq!(walk.sim.get::<Destination>(walker).get(), None);
    assert!(walk.get_route(walker).arrived_short_of(goal));
    // Sent there again, as an order would, it plans nothing, and stays with no destination.
    let entity = walk.sim.entity(walker);
    let changed = |walk: &Walk| {
        walk.sim
            .world
            .entity(entity)
            .get_ref::<Route>()
            .unwrap()
            .last_changed()
    };
    let before = changed(&walk);
    walk.sim
        .world
        .get_mut::<Destination>(entity)
        .unwrap()
        .set(Some(goal));
    walk.sim.step();
    assert_eq!(changed(&walk), before);
    assert_eq!(walk.sim.get::<Destination>(walker).get(), None);
    assert_eq!(*walk.sim.get::<Position>(walker), short);
    // The middle tower dies, and the walker goes on through its cell from the next tick: 3 m to
    // the gap, (4.5, 1.5), then 2 m on, in 3 ticks. Its route reached the goal, so it forgets the
    // route, and its progress.
    walk.sim.world.entity_mut(middle).insert(Dead);
    walk.sim.step();
    assert_eq!(walk.sim.get::<Destination>(walker).get(), Some(goal));
    for _ in 0..2 {
        walk.sim.step();
    }
    assert_eq!(*walk.sim.get::<Position>(walker), goal);
    assert_eq!(walk.sim.get::<Destination>(walker).get(), None);
    assert_eq!(walk.get_route(walker).goal(), None);
    assert_eq!(*walk.sim.get::<Progress>(walker), Progress::default());
}

/// A corridor 10 m by 4 m in half-meter cells, a lane along z = 2 from (1, 2) to (9, 2), towers
/// at `towers`, a creep on the west spawn at (1, 1), which walks and so blocks nothing, and a camp
/// marker at `camp`.
fn corridor(towers: &[(i64, i64)], camp: (i64, i64)) -> MapData {
    let point = MapPoint::ground;
    let placed = |unit_type, (x, z)| PlacedUnitData::new(unit_type, "west", point(x, z));
    let marker = |name, (x, z)| MarkerData::tagged(name, &[name], point(x, z));
    MapData {
        navigation: Some(MapNavigationData {
            cell: Scalar::Decimal(Num::HALF),
            walls: Vec::new(),
        }),
        paths: vec![PathData {
            name: DeclaredName::new("lane").unwrap(),
            points: vec![point(1, 2), point(9, 2)],
        }],
        units: towers
            .iter()
            .map(|&at| placed("tower", at))
            .chain([placed("creep", (1, 1))])
            .collect(),
        markers: vec![marker("spawn", (1, 1)), marker("camp", camp)],
        ..MapData::planar(
            Bounds::new([Num::int(0), Num::int(0)], [Num::int(10), Num::int(4)]).unwrap(),
        )
    }
}

/// The corridor's bodies: a tower of 0.9 m on the ground, a cloud of the same width in the air.
fn corridor_body(unit_type: &str) -> Option<Body> {
    let tower = Body::new(Num::from_bits((9 << Num::FRAC_BITS) / 10)).unwrap();
    match unit_type {
        "tower" => Some(tower),
        "cloud" => Some(tower.on(AIR)),
        _ => None,
    }
}

#[test]
fn a_map_loads_only_if_the_widest_walker_reaches_every_waypoint_and_stands_on_every_marker() {
    // Walkers of 0.5 m in the corridor. Towers at (5, 0) and (5, 4) block the centers closer than
    // 1.4 m, z up to 1.25 and from 2.75 at x = 4.75 and 5.25, which leaves z = 1.75 and 2.25 open:
    // a gap a walker passes. One more at (5, 2) closes it.
    let half = Num::HALF;
    let point = MapPoint::ground;
    let placed = |unit_type, (x, z)| PlacedUnitData::new(unit_type, "west", point(x, z));
    let map = corridor;
    let body_of = corridor_body;
    let rules = NavigationRules::default();
    let check = |towers: &[(i64, i64)], neutral| {
        map(towers, neutral).check_walkable(&[ground(half)], &rules, body_of)
    };
    assert_eq!(check(&[(5, 0), (5, 4)], (8, 3)), Ok(()));
    let unreachable = MapProblem::WaypointUnreachable {
        path: DeclaredName::new("lane").unwrap(),
        waypoint: 1,
    };
    assert_eq!(check(&[(5, 0), (5, 2), (5, 4)], (8, 3)), Err(unreachable));
    // A tower 1 m from the lane's end, (9, 2), or from the spawn marker, (1, 1), or from the camp
    // marker, (8, 3): closer than 1.4 m.
    let blocked = MapProblem::WaypointBlocked {
        path: DeclaredName::new("lane").unwrap(),
        waypoint: 1,
    };
    assert_eq!(check(&[(9, 3)], (8, 1)), Err(blocked));
    let blocked = |marker: &str| {
        let marker = DeclaredName::new(marker).unwrap();
        Err(MapProblem::MarkerBlocked { marker })
    };
    assert_eq!(check(&[(2, 1)], (8, 3)), blocked("spawn"));
    assert_eq!(check(&[(7, 3)], (8, 3)), blocked("camp"));

    // Each layer's widest walker is checked against the bodies of its layer alone: an air walker
    // passes over the towers that close the lane, and a cloud by the camp blocks it there, but
    // not a walker on the ground.
    let flyer = Walker {
        layer: AIR,
        radius: half,
    };
    let both = [ground(half), flyer];
    let closed = map(&[(5, 0), (5, 2), (5, 4)], (8, 3));
    assert_eq!(closed.check_walkable(&[flyer], &rules, body_of), Ok(()));
    assert_eq!(
        closed.check_walkable(&both, &rules, body_of),
        Err(MapProblem::WaypointUnreachable {
            path: DeclaredName::new("lane").unwrap(),
            waypoint: 1,
        })
    );
    let mut clouded = map(&[(5, 0), (5, 4)], (8, 3));
    clouded.units.push(placed("cloud", (7, 3)));
    assert_eq!(
        clouded.check_walkable(&[ground(half)], &rules, body_of),
        Ok(())
    );
    assert_eq!(
        clouded.check_walkable(&both, &rules, body_of),
        blocked("camp")
    );
    assert_eq!(clouded.check_walkable(&[], &rules, body_of), Ok(()));
}

#[test]
fn a_wall_closes_a_way_and_loads_only_as_a_simple_polygon_on_a_declared_layer() {
    let half = Num::HALF;
    let point = MapPoint::ground;
    let rules = NavigationRules::default();
    let flyer = Walker {
        layer: AIR,
        radius: half,
    };
    let both = [ground(half), flyer];
    let body_of = corridor_body;
    // A wall on the ground across the map from x = 5 to 6 closes the lane as the towers do, and
    // the air walker flies over it; a wall round the spawn marker leaves the walker no place
    // there. A wall's points must be in the map's bounds, on a layer the mode declares, and make
    // a simple polygon.
    let wall = |layer: Option<&str>, points: &[(i64, i64)]| WallData {
        layer: layer.map(|layer| DeclaredName::new(layer).unwrap()),
        points: points.iter().map(|&(x, z)| point(x, z)).collect(),
    };
    let walled = |walls: Vec<WallData>| {
        let mut walled = corridor(&[], (8, 3));
        walled.navigation.as_mut().unwrap().walls = walls;
        walled
    };
    let across = walled(vec![wall(None, &[(5, 0), (6, 0), (6, 4), (5, 4)])]);
    assert_eq!(across.check_walkable(&[flyer], &rules, body_of), Ok(()));
    assert_eq!(
        across.check_walkable(&both, &rules, body_of),
        Err(MapProblem::WaypointUnreachable {
            path: DeclaredName::new("lane").unwrap(),
            waypoint: 1,
        })
    );
    let round = walled(vec![wall(None, &[(0, 0), (2, 0), (2, 2), (0, 2)])]);
    let spawn = DeclaredName::new("spawn").unwrap();
    let blocked = Err(MapProblem::MarkerBlocked { marker: spawn });
    assert_eq!(round.check_walkable(&both, &rules, body_of), blocked);
    let air = DeclaredName::new("air").unwrap();
    let layered = NavigationRules {
        layers: vec![DeclaredName::new("ground").unwrap(), air.clone()],
    };
    let refused = [
        (
            wall(Some("air"), &[(5, 0), (6, 0), (6, 4)]),
            &rules,
            ModeError::UnknownLayer(air),
        ),
        (
            wall(None, &[(5, 0), (6, 0), (6, 5)]),
            &rules,
            ModeError::OutOfBounds,
        ),
        (
            wall(None, &[(5, 0), (6, 0)]),
            &rules,
            ModeError::Wall {
                wall: 0,
                problem: PolygonError::TooFewPoints,
            },
        ),
        (
            wall(Some("air"), &[(5, 0), (6, 4), (6, 0), (5, 4)]),
            &layered,
            ModeError::Wall {
                wall: 0,
                problem: PolygonError::EdgesMeet {
                    first: 0,
                    second: 2,
                },
            },
        ),
    ];
    for (wall, rules, error) in refused {
        assert_eq!(walled(vec![wall]).walls(rules), Err(error));
    }
    let lifted = walled(vec![wall(Some("air"), &[(5, 0), (6, 0), (6, 4)])]);
    let walls = lifted.walls(&layered).unwrap();
    assert_eq!(
        walls.iter().map(|wall| wall.layer).collect::<Vec<_>>(),
        [AIR]
    );
}

#[test]
fn a_dead_unit_forgets_its_destination_and_a_stopped_one_keeps_it() {
    let mut walk = Walk::new();
    let unit = walk.unit(at(0, 0, 0), Some(at(0, 0, 5)));
    walk.sim.insert(unit, Dead);
    // Tags that block moving hold their unit where it stands, its destination kept, until they
    // end; tags that block anything else let it walk.
    let cases = [
        (&[Block::Move, Block::Attack][..], false),
        (&[Block::Move][..], false),
        (&[Block::Attack, Block::Cast][..], true),
    ];
    let walkers: Vec<_> = (2..)
        .step_by(2)
        .zip(cases)
        .map(|(x, (blocks, _))| {
            let id = walk.unit(at(x, 0, 0), Some(at(x, 0, 5)));
            walk.sim.set_blocks(id, blocks);
            (id, x)
        })
        .collect();
    walk.sim.step();
    assert_eq!(*walk.sim.get::<Position>(unit), at(0, 0, 0));
    assert_eq!(walk.sim.get::<Destination>(unit).get(), None);
    for (&(id, x), (blocks, walks)) in walkers.iter().zip(cases) {
        let z = i64::from(walks);
        assert_eq!(*walk.sim.get::<Position>(id), at(x, 0, z), "{blocks:?}");
        assert_eq!(
            walk.sim.get::<Destination>(id).get(),
            Some(at(x, 0, 5)),
            "{blocks:?}"
        );
        walk.sim.set_blocks(id, &[]);
    }
    // The blocks end: each walks on a meter from where it stood.
    walk.sim.step();
    for (&(id, x), (blocks, walks)) in walkers.iter().zip(cases) {
        let z = i64::from(walks) + 1;
        assert_eq!(*walk.sim.get::<Position>(id), at(x, 0, z), "{blocks:?}");
    }
}

#[test]
fn every_navigation_type_is_state() {
    let mut walk = Walk::new();
    let unit = walk.unit(at(0, 0, 0), Some(at(0, 0, 5)));
    let entity = walk.sim.entity(unit);
    walk.sim.insert(
        unit,
        (
            PathWalker::start(PathEnd::Start),
            OnPath::new(PathId::new(0)),
        ),
    );
    let mut route = walk.sim.world.get_mut::<Route>(entity).unwrap();
    route.ask(at(3, 0, 4), Tick::new(2));
    let lane = || Paths::new([("lane", &[at(0, 0, 0), at(0, 0, 5)][..])]);
    walk.sim.world.insert_resource(lane());
    // A restore loads the map first, as the packages give it.
    let mut restored = Walk::new();
    restored.sim.world.insert_resource(lane());
    walk.sim.restore_into(&mut restored.sim);
}

mod forced;
