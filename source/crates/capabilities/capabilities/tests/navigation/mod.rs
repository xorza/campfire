use bevy_ecs::change_detection::DetectChanges;
use campfire_capabilities::internals::*;
use campfire_capabilities::{Match, Release};
use campfire_common::Tick;
use campfire_declarations::Role;
use campfire_declarations::internals::BookLoad;
use campfire_geometry::{Bounds, Polygon, Shape};
use campfire_math::{Num, Vec3};
use campfire_navigation::internals::PlannedRoute;
use campfire_navigation::*;
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{Capability, Position, SimWorld, StableId, Unpredicted};
use campfire_units::{
    ArrivedShort, AtWork, Body, BodyForm, BodyShape, Dead, Destination, Facing, Layer, MapBook,
    TurnStep, Units, Wall,
};
use campfire_values::{Block, PathId, Team};

const ONE: i64 = 1 << 24;

/// The second layer, as an RTS's air.
const AIR: Layer = Layer::new(1);

/// A walker of `radius` on the first layer.
const fn ground(radius: Num) -> Walker {
    Walker {
        layer: Layer::FIRST,
        radius,
    }
}

/// A match of navigation, with the type its units are of.
fn walk_match() -> Match {
    walk_match_of(&[Capability::Navigation])
}

/// A match of `declared`, with navigation among them, as `walk_match` gives one.
fn walk_match_of(declared: &[Capability]) -> Match {
    let mut game = Match::builder(declared).role(Role::Rules).build();
    Navigation::walker_type(&mut game);
    game
}

/// A match of navigation and of stats, whose modifiers block units.
fn blocking_walk_match() -> Match {
    walk_match_of(&[Capability::Stats, Capability::Navigation])
}

#[test]
fn a_unit_walks_to_its_destination_exactly() {
    let mut game = walk_match();
    let straight = Navigation::unit(&mut game, (0, 0, 0), Some(Vec3::from((0, 0, 5))));
    // Offset (3, 0, 4), distance 5: the first meter is 3/5 and 4/5, each rounded once:
    // 10 066 329.6 → 10 066 330 and 13 421 772.8 → 13 421 773. The height stays at y = 2.
    let slanted = Navigation::unit(&mut game, (4, 2, 0), Some(Vec3::from((7, 2, 4))));
    let still = Navigation::unit(&mut game, (9, 0, 9), None);

    game.step();
    assert_eq!(game.part::<Position>(straight).get(), Vec3::from((0, 0, 1)));
    assert_eq!(
        game.part::<Position>(slanted).get(),
        Vec3::new(
            Num::from_bits(4 * ONE + 10_066_330),
            Num::int(2),
            Num::from_bits(13_421_773)
        )
    );
    assert_eq!(game.part::<Position>(still).get(), Vec3::from((9, 0, 9)));
    // With no turn rate, each faces its way at once: along `+z`, 270°; the one that stands keeps
    // its spawn's 0°.
    assert_eq!(game.part::<Facing>(straight).degrees(), Num::int(270));
    assert_eq!(game.part::<Facing>(still).degrees(), Num::ZERO);
    // Straight along z, one meter a tick: done in the fifth tick, and the destination dropped.
    for z in 2..=5 {
        game.step();
        assert_eq!(game.part::<Position>(straight).get(), Vec3::from((0, 0, z)));
    }
    assert_eq!(game.part::<Destination>(straight).get(), None);
    game.step();
    assert_eq!(game.part::<Position>(straight).get(), Vec3::from((0, 0, 5)));
}

#[test]
fn a_walker_turns_at_its_turn_rate_and_walks_once_it_faces_its_way() {
    // From 0°, along `x`, to a goal along `−z`, 90°, at 3° a tick: it stands while more than
    // 11.25° is left, through tick 26, at 78°, 12° short; in tick 27 it turns to 81°, 9° short,
    // and walks its meter; then it turns on as it walks, to 90° in tick 30.
    let mut game = walk_match();
    let walker = Navigation::unit(&mut game, (0, 0, 0), Some(Vec3::from((0, 0, -10))));
    game.insert_parts(walker, TurnStep::new(Num::int(3)).unwrap());
    for tick in 1..=26 {
        game.step();
        assert_eq!(game.part::<Facing>(walker).degrees(), Num::int(3 * tick));
        assert_eq!(
            game.part::<Position>(walker).get(),
            Vec3::from((0, 0, 0)),
            "tick {tick}"
        );
    }
    game.step();
    assert_eq!(game.part::<Facing>(walker).degrees(), Num::int(81));
    assert_eq!(game.part::<Position>(walker).get(), Vec3::from((0, 0, -1)));
    for _ in 28..=30 {
        game.step();
    }
    assert_eq!(game.part::<Facing>(walker).degrees(), Num::int(90));
    assert_eq!(game.part::<Position>(walker).get(), Vec3::from((0, 0, -4)));
}

#[test]
fn every_unit_that_walks_ends_the_tick_within_the_bounds() {
    let mut game = walk_match();
    assert_eq!(game.resource::<MapBook>().bounds, Bounds::WORLD);
    MapBook::amend(&mut game, |_, map| {
        map.bounds = Bounds::new([Num::int(-4), Num::int(-4)], [Num::int(4), Num::int(4)]).unwrap();
    });
    // Outside the bounds at x = 6: clamped to the edge x = 4, at its height and its z.
    let outside = Navigation::unit(&mut game, (6, 1, 2), None);
    // From x = 3 a meter towards x = 5: it arrives at 4, then walks on to 5 and back to 4.
    let past = Navigation::unit(&mut game, (3, 0, 0), Some(Vec3::from((5, 0, 0))));
    let inside = Navigation::unit(&mut game, (1, 0, 1), None);
    let id = Navigation::walker_at(&mut game, (9, 0, 9)).place(&mut game);

    game.step();
    assert_eq!(game.part::<Position>(outside).get(), Vec3::from((4, 1, 2)));
    assert_eq!(game.part::<Position>(past).get(), Vec3::from((4, 0, 0)));
    assert_eq!(game.part::<Position>(inside).get(), Vec3::from((1, 0, 1)));
    game.step();
    assert_eq!(game.part::<Position>(past).get(), Vec3::from((4, 0, 0)));
    assert_eq!(game.part::<Destination>(past).get(), None);
    // A unit that does not walk is not moved: the map's check keeps it within the bounds.
    assert_eq!(game.part::<Position>(id).get(), Vec3::from((9, 0, 9)));
}

#[test]
fn bodies_part_and_block_the_way() {
    // Bodies of 0.5 m walking a quarter meter a tick. A from x = 0 and B from x = 3 walk at each
    // other: 3 − 0.5 t apart after tick t, and in tick 5, at 1.25 and 1.75, they overlap by half
    // a meter, which parts them a quarter meter each, to 1 and 2: 1 m apart, the sum of their
    // radii. Every tick after, they walk in and part again, to the same places. The mode moves no
    // unit, so no push parts them before they touch.
    let quarter = Num::QUARTER;
    let half = Num::HALF;
    let mut game = blocking_walk_match();
    Navigation::load_moves(&mut game, Moves::None);
    let a = Navigation::walker_at(&mut game, (0, 0, 0))
        .circle(half)
        .walks(quarter, Some(Vec3::from((10, 0, 0))))
        .place(&mut game);
    let b = Navigation::walker_at(&mut game, (3, 0, 0))
        .circle(half)
        .walks(quarter, Some(Vec3::from((-10, 0, 0))))
        .place(&mut game);
    // A tower of radius 1 at (−6, 4), which does not walk, and C walking at it along z from 0: C
    // touches it at z = 2.5, 1.5 m off, and in tick 11, at 2.75, is pushed back all the way.
    let tower = Navigation::walker_at(&mut game, (-6, 0, 4))
        .circle(Num::ONE)
        .place(&mut game);
    let c = Navigation::walker_at(&mut game, (-6, 0, 0))
        .circle(half)
        .walks(quarter, Some(Vec3::from((-6, 0, 8))))
        .place(&mut game);
    // D walks through a dead body on its way.
    let dead = Navigation::walker_at(&mut game, (20, 0, 0))
        .circle(half)
        .walks(quarter, None)
        .place(&mut game);
    game.insert_parts(dead, Dead);
    let d = Navigation::walker_at(&mut game, (18, 0, 0))
        .circle(half)
        .walks(quarter, Some(Vec3::from((22, 0, 0))))
        .place(&mut game);
    // A walker at a rooted unit on its way along z = 10, which so stands: the walker touches it
    // in tick 8, at x = 1, and from tick 9 takes the whole overlap, back to 1 each tick, as a
    // walker takes it from a unit that stands. The rooted unit keeps its place and destination.
    let rooted = Navigation::walker_at(&mut game, (0, 0, 10))
        .circle(half)
        .walks(quarter, Some(Vec3::from((10, 0, 10))))
        .place(&mut game);
    Units::set_blocks(&mut game, rooted, &[Block::Move]);
    let walker = Navigation::walker_at(&mut game, (3, 0, 10))
        .circle(half)
        .walks(quarter, Some(Vec3::from((-10, 0, 10))))
        .place(&mut game);
    for _ in 0..5 {
        game.step();
    }
    assert_eq!(
        [a, b].map(|id| game.part::<Position>(id).get()),
        [Vec3::from((1, 0, 0)), Vec3::from((2, 0, 0))]
    );
    for _ in 5..16 {
        game.step();
    }
    assert_eq!(
        [a, b].map(|id| game.part::<Position>(id).get()),
        [Vec3::from((1, 0, 0)), Vec3::from((2, 0, 0))]
    );
    let c_at = Vec3::new(Num::int(-6), Num::ZERO, Num::int(2) + half);
    assert_eq!(
        [c, tower].map(|id| game.part::<Position>(id).get()),
        [c_at, Vec3::from((-6, 0, 4))]
    );
    // D, 16 ticks on at a quarter meter, passed the dead body at 20 to reach 22.
    assert_eq!(
        [d, dead].map(|id| game.part::<Position>(id).get()),
        [Vec3::from((22, 0, 0)), Vec3::from((20, 0, 0))]
    );
    assert_eq!(
        [rooted, walker].map(|id| game.part::<Position>(id).get()),
        [Vec3::from((0, 0, 10)), Vec3::from((1, 0, 10))]
    );
    assert_eq!(
        game.part::<Destination>(rooted).get(),
        Some(Vec3::from((10, 0, 10)))
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
    let mut game = walk_match();
    let blocked = Navigation::walker_at(&mut game, (0, 0, 0))
        .circle(half)
        .walks(quarter, Some(Vec3::from((4, 0, 0))))
        .place(&mut game);
    let passing = Navigation::walker_at(&mut game, (0, 0, 4))
        .circle(half)
        .walks(quarter, Some(Vec3::from((4, 0, 4))))
        .place(&mut game);
    let fixed = Navigation::walker_at(&mut game, (2, 0, 0))
        .circle(half)
        .place(&mut game);
    let resting = Navigation::walker_at(&mut game, (2, 0, 4))
        .circle(half)
        .walks(quarter, None)
        .place(&mut game);
    // As on a client: the sim runs on no held unit, and collision names them.
    Unpredicted::register(&mut game);
    for held in [fixed, resting] {
        game.insert_parts(held, Unpredicted);
    }
    for _ in 0..16 {
        game.step();
    }
    let places = [blocked, passing, fixed, resting].map(|id| game.part::<Position>(id).get());
    assert_eq!(
        places,
        [
            Vec3::from((1, 0, 0)),
            Vec3::from((4, 0, 4)),
            Vec3::from((2, 0, 0)),
            Vec3::from((2, 0, 4))
        ]
    );
}

#[test]
fn the_pathing_grid_takes_a_body_as_it_spawns_and_its_changes_from_the_next_tick() {
    // 1 m cells over (−2, −2) to (2, 2), for walkers of 0.5 m: a tower of 0.5 m at (−1.5, −1.5)
    // blocks its own cell, 0, whose center is on it; one the client only holds, at (1.5, 1.5),
    // blocks cell 15. Each blocks as it spawns, as navigation's spawn callback gives the grid
    // the body (walkers-get-through.md, A spawn stands where it can walk). A walker never marks
    // the grid.
    let half = Num::HALF;
    let mut game = walk_match();
    Navigation::load_open_pathing(&mut game, Num::ONE, [-2, -2], [2, 2], [ground(half)]);
    let quarter = |value: i64| Num::from_bits(value << 22);
    let place = |x: i64, z: i64| Vec3::new(quarter(x), Num::ZERO, quarter(z));
    let blocked = |game: &Match| {
        (0..16)
            .filter(|&cell| !Navigation::cell_open(game, ground(half), cell))
            .collect::<Vec<_>>()
    };
    let tower = Navigation::walker_at(&mut game, place(-6, -6))
        .circle(half)
        .place(&mut game);
    let held = Navigation::walker_at(&mut game, place(6, 6))
        .circle(half)
        .place(&mut game);
    Navigation::walker_at(&mut game, place(0, 0))
        .circle(half)
        .walks(Num::ONE, Some(Vec3::from((0, 0, 1))))
        .place(&mut game);
    Unpredicted::register(&mut game);
    game.insert_parts(held, Unpredicted);
    assert_eq!(blocked(&game), [0, 15]);
    game.step();
    assert_eq!(blocked(&game), [0, 15]);
    // A tower that died stands no more in the way, from the next tick.
    game.insert_parts(tower, Dead);
    game.step();
    assert_eq!(blocked(&game), [15]);
    // In one tick the tower lives again and the held one dies: as many static bodies, none of
    // them moved, and the grid follows.
    let held_entity = game.unit_entity(held);
    game.entity_mut(held_entity).insert(Dead);
    let tower_entity = game.unit_entity(tower);
    game.entity_mut(tower_entity).remove::<Dead>();
    game.step();
    assert_eq!(blocked(&game), [0]);
    // The tower moves to (−0.5, 1.5): column 1 of row 3, cell 13. Then it goes.
    *game.part_mut::<Position>(tower) = Position::from(place(-2, 6));
    game.step();
    assert_eq!(blocked(&game), [13]);
    game.entity_mut(tower_entity).despawn();
    game.step();
    assert_eq!(blocked(&game), Vec::<usize>::new());
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
        let mut game = walk_match();
        if planned {
            Navigation::load_open_pathing(&mut game, half, [-8, -8], [8, 8], [ground(half)]);
        }
        Navigation::walker_at(&mut game, (0, 0, 0))
            .circle(tower_radius)
            .place(&mut game);
        let walker = Navigation::walker_at(&mut game, (-4, 0, 0))
            .circle(half)
            .walks(quarter, Some(Vec3::from((4, 0, 0))))
            .place(&mut game);
        let mut closest = u128::MAX;
        let mut arrived = None;
        for tick in 0..80 {
            game.step();
            let offset = game.part::<Position>(walker).get();
            closest = closest.min(offset.length_squared_bits());
            if arrived.is_none() && game.part::<Destination>(walker).get().is_none() {
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
            assert_eq!(game.part::<Position>(walker).get(), Vec3::from((4, 0, 0)));
        } else {
            // It stops against the tower, the two radii from its center, pressed there.
            assert_eq!(arrived, None);
            let stop = Vec3::new(-(tower_radius + half), Num::ZERO, Num::ZERO);
            assert_eq!(game.part::<Position>(walker).get(), stop);
        }
    }
}

#[test]
fn a_walker_goes_round_a_wall_smoothed_past_its_corners_and_never_through_it() {
    // Over 1 m cells from (0, 0) to (10, 6), a wall from (4, 0) to (6, 4) blocks the cells whose
    // centers it holds, columns 4 and 5 of rows 0 to 3, to a walker of no body. One at (2, 1)
    // walks half a meter a tick to (8, 1).
    let half = Num::HALF;
    let mut game = walk_match();
    let box_of = |[x0, z0]: [i64; 2], [x1, z1]: [i64; 2]| {
        let area = [[x0, z0], [x1, z0], [x1, z1], [x0, z1]].map(|point| point.map(Num::int));
        Polygon::new(area).unwrap()
    };
    let barrier = Wall {
        layer: Layer::FIRST,
        area: box_of([4, 0], [6, 4]),
    };
    Navigation::load_walled(
        &mut game,
        Num::ONE,
        [0, 0],
        [10, 6],
        vec![ground(Num::ZERO)],
        &[barrier],
    );
    let walker = Navigation::unit(&mut game, (2, 0, 1), Some(Vec3::from((8, 0, 1))));
    // Its route goes over the wall's top, row 4, and keeps a waypoint only where the line on would
    // touch a blocked cell: from (2, 1) to the next cell, (4.5, 4.5), it is at z = 3.8 at x = 4,
    // on cell (4, 3); from (3.5, 4.5) to the cell after (6.5, 4.5), (7.5, 3.5), at z = 3.875 at
    // x = 6, on cell (5, 3). From (6.5, 4.5) it goes straight to its goal.
    let point = |x: i64, z: i64| Vec3::new(half * x, Num::ZERO, half * z);
    game.step();
    let corners = [point(7, 9), point(13, 9), point(16, 2)];
    assert_eq!(Navigation::ahead(&game, walker), corners);
    // It walks √14.5 ≈ 3.81 m, 3 m and √14.5 m, a meter a tick, never into the wall's square,
    // and arrives in its eleventh tick.
    let mut steps = 1;
    while game.part::<Destination>(walker).get().is_some() {
        let position = game.part::<Position>(walker).get();
        let inside =
            |value: Num, low: i64, high: i64| Num::int(low) <= value && value <= Num::int(high);
        assert!(
            !(inside(position.x, 4, 6) && inside(position.z, 0, 4)),
            "{position:?}"
        );
        game.step();
        steps += 1;
    }
    assert_eq!(steps, 11);
    assert_eq!(game.part::<Position>(walker).get(), Vec3::from((8, 0, 1)));
}

/// Whether the bodies of radii `a` and `b` at `first` and `second` overlap on the ground plane,
/// exactly.
fn overlap(first: Vec3, a: Num, second: Vec3, b: Num) -> bool {
    let reach = u128::from((a + b).to_bits().unsigned_abs());
    first.ground_offset(second).length_squared_bits() < reach * reach
}

#[test]
fn a_walker_goes_round_units_that_stand_in_its_way() {
    // A hero of 0.5 m stands at the origin and a creep of 0.35 m at (2, 0.5), on the line of a
    // walker of 0.5 m from (−4, 0) to (4, 0), a quarter meter a tick, over half-meter cells. It
    // goes round both, touching neither, and neither moves, in a mode that moves no unit; with no
    // pathing grid it walks into the hero, yields to it, and stays pressed there.
    let half = Num::HALF;
    let quarter = Num::QUARTER;
    let creep_radius = Num::from_bits((35 << Num::FRAC_BITS) / 100);
    let creep_at = Vec3::new(Num::int(2), Num::ZERO, half);
    for planned in [true, false] {
        let mut game = walk_match();
        Navigation::load_moves(&mut game, Moves::None);
        if planned {
            Navigation::load_open_pathing(
                &mut game,
                half,
                [-8, -8],
                [8, 8],
                vec![ground(creep_radius), ground(half)],
            );
        }
        let hero = Navigation::walker_at(&mut game, (0, 0, 0))
            .circle(half)
            .walks(quarter, None)
            .place(&mut game);
        let creep = Navigation::walker_at(&mut game, creep_at)
            .circle(creep_radius)
            .walks(quarter, None)
            .place(&mut game);
        let walker = Navigation::walker_at(&mut game, (-4, 0, 0))
            .circle(half)
            .walks(quarter, Some(Vec3::from((4, 0, 0))))
            .place(&mut game);
        let mut touched = false;
        let mut arrived = None;
        for tick in 0..80 {
            game.step();
            let position = game.part::<Position>(walker).get();
            touched |= overlap(position, half, Vec3::from((0, 0, 0)), half);
            touched |= overlap(position, half, creep_at, creep_radius);
            if arrived.is_none() && game.part::<Destination>(walker).get().is_none() {
                arrived = Some(tick);
            }
        }
        let still = [
            game.part::<Position>(hero).get(),
            game.part::<Position>(creep).get(),
        ];
        if planned {
            assert!(!touched);
            // Three ticks more than the 32 a straight 8 m takes: its long route goes round both
            // from the start, as the cells they hold cost more (04-capabilities/navigation.md, A
            // route prices the units on it).
            assert_eq!(arrived, Some(35));
            assert_eq!(game.part::<Position>(walker).get(), Vec3::from((4, 0, 0)));
            assert_eq!(still, [Vec3::from((0, 0, 0)), creep_at]);
        } else {
            // Against the hero, the two radii, 1 m, from its center.
            assert_eq!(arrived, None);
            assert_eq!(game.part::<Position>(walker).get(), Vec3::from((-1, 0, 0)));
        }
    }
}

#[test]
fn an_idle_friend_on_a_walkers_way_steps_aside_before_it_stalls_and_an_enemy_or_a_busy_one_does_not()
 {
    // Half-meter cells over ±12 m; walls of 10 by 4 m at (−5, ±3) leave a corridor 2 m wide, z −1
    // to 1, for x below 0. A unit of 0.5 m stands at (0.5, 0), at the corridor's mouth, and a
    // walker of 0.5 m of team 0 walks from (−8, 0) to (6, 0), a quarter meter a tick. Its long
    // route goes round the cells the unit holds, which cost more (04-capabilities/navigation.md, A
    // route prices the units on it), so it leaves the mouth on the unit's +z side. A friend of team
    // 0 that stands idle steps off the walker's way once it is on the stretch the walker's window
    // reaches, 8 cells, 4 m, and the two radii: in tick 31, near the mouth, to −z, the side it
    // stands on from that stretch, and the walker, which never stops, is done in tick 58
    // (04-capabilities/navigation.md, An idle friend steps aside). Units that drive do the same,
    // and take no push: the walker is done in tick 59 (04-capabilities/navigation.md, A unit that
    // drives yields by driving). An enemy stays, and so does a friend at work. The walker the
    // enemy holds is stuck once, and the route it asks walls the enemy's cells and goes round the
    // corridor's walls: one stall, and it arrives in tick 202 (walkers-get-through.md, A stuck
    // walker's route walls the units of other teams that stand). The one the friend at work holds
    // never arrives: it comes no nearer its waypoint from tick 33, and, stuck 30 ticks, 1 s, it
    // asks its route again in tick 63, answered as the next tick starts, whose waypoint it takes
    // fresh, and again each 31 ticks, whatever detour it takes (walkers-get-through.md, A walker
    // progresses only when it comes nearer its waypoint); the sixth time, past its 5 asks, it stops
    // where it stands, in tick 63 + 5 · 31 = 218, with no destination and no goal to arrive short
    // of (04-capabilities/navigation.md, A stuck walker plans again, then stops): 6 stalls, which
    // navigation's report counts, against none of a walker the friend steps aside for.
    let half = Num::HALF;
    let blocker_at = Vec3::new(half, Num::ZERO, Num::ZERO);
    let goal = Vec3::from((6, 0, 0));
    let run = |team: u8, at_work: bool, gait: Gait| {
        let mut game = walk_match();
        Navigation::load_open_pathing(&mut game, half, [-12, -12], [12, 12], [ground(half)]);
        let walker_type = Navigation::walker_type(&mut game);
        let navigation = UnitNavigationData::new(gait, Num::ONE).unwrap();
        Navigation::load_unit_navigation(&mut game, walker_type, navigation);
        let wall = BodyForm::new(
            BodyShape::box_sized([Num::int(10), Num::int(4)]).unwrap(),
            Layer::FIRST,
        );
        for z in [-3, 3] {
            Navigation::walker_at(&mut game, (-5, 0, z))
                .body(wall)
                .place(&mut game);
        }
        game.step();
        let blocker = Navigation::walker_at(&mut game, blocker_at)
            .team(Team::new(team))
            .circle(half)
            .walks(Num::QUARTER, None)
            .place(&mut game);
        game.insert_parts(blocker, AtWork::new(at_work));
        let walker = Navigation::walker_at(&mut game, (-8, 0, 0))
            .team(Team::new(0))
            .circle(half)
            .walks(Num::QUARTER, Some(goal))
            .place(&mut game);
        let (mut done, mut aside, mut stalls) = (None, None, 0);
        for tick in 0..240 {
            game.step();
            stalls += game.resource::<StalledWalkers>().get();
            if done.is_none() && game.part::<Destination>(walker).get().is_none() {
                done = Some(tick + 1);
            }
            if aside.is_none() && game.part::<Destination>(blocker).get().is_some() {
                aside = Some(tick + 1);
            }
        }
        let at = [walker, blocker].map(|unit| game.part::<Position>(unit).get());
        (
            at,
            (done, aside, stalls),
            *game.part::<ArrivedShort>(walker),
        )
    };
    // Where the friend ends, in bits of 2⁻²⁴: the one that steps, also pushed on by the walker
    // as it passes within their personal spaces, at (0.50923, −0.83157); the one that drives, on
    // its place aside, the two radii off the walker's way, at (0.5, −0.75).
    let ends = [
        (Gait::Step, 58, [8543407, 0, -13951395]),
        (Gait::Drive, 59, [8388608, 0, -12582912]),
    ];
    for (gait, ended, at) in ends {
        let ([walker, friend], (done, aside, stalls), short) = run(0, false, gait);
        assert_eq!(
            (walker, done, aside, stalls, short),
            (goal, Some(ended), Some(31), 0, ArrivedShort::of(None)),
            "{gait:?}"
        );
        assert_eq!(friend.to_bits(), at, "{gait:?}");
    }
    // The walker round the enemy ends on its goal. The friend at work costs it more, so its route
    // goes round the friend's cells, and the walker presses at the mouth's corner, against the
    // corridor's wall a bit short of z = 0.5, touching the friend 30° off the line, at x within
    // bits of 0.5 − √3/2; once stopped, pinned by the wall, it leaves the few bits of overlap the
    // rounding left to the friend.
    let held = [
        (1, false, [[100663296, 0, 0], [8388608, 0, 0]], 202, 1),
        (0, true, [[-6140873, 0, 8388607], [8388620, 0, -7]], 218, 6),
    ];
    for (team, at_work, at, ended, asked) in held {
        let ([walker, blocker], (done, aside, stalls), short) = run(team, at_work, Gait::Step);
        assert_eq!(
            (done, aside, stalls, short),
            (Some(ended), None, asked, ArrivedShort::of(None)),
            "{team}"
        );
        assert_eq!([walker, blocker].map(Vec3::to_bits), at, "{team}");
    }
}

#[test]
fn a_walker_whose_goal_a_standing_unit_crowds_arrives_once_stuck_there() {
    // A walker of 0.5 m of team 0 walks from (−4, 0) to (0.4, 0), a quarter meter a tick, and a
    // friend of 4 m at work, which takes no push, stands at (5, 0): the goal lies 4.6 m off it,
    // past their two radii, 4.5 m, but within their personal space of 5/4 of those, 5.625 m, and
    // a push of a quarter of the walker's way into that space, more than its step there, takes
    // it back each tick it steps nearer. It stands at x = 0.0973, in bits 1,632,142, 0.30 m short
    // of the goal, within the friend's personal space as the goal is, and, stuck there 5 ticks,
    // 150 ms, it has arrived at the goal in tick 26, with no stall and not short
    // (walkers-get-through.md, A walker stuck by a crowded goal has arrived).
    let half = Num::HALF;
    let goal = Vec3::new(Num::int(2) / 5, Num::ZERO, Num::ZERO);
    let mut game = walk_match();
    Navigation::load_open_pathing(&mut game, half, [-12, -12], [12, 12], [ground(half)]);
    let friend = Navigation::walker_at(&mut game, (5, 0, 0))
        .team(Team::new(0))
        .circle(Num::int(4))
        .walks(Num::QUARTER, None)
        .place(&mut game);
    game.insert_parts(friend, AtWork::new(true));
    game.step();
    let walker = Navigation::walker_at(&mut game, (-4, 0, 0))
        .team(Team::new(0))
        .circle(half)
        .walks(Num::QUARTER, Some(goal))
        .place(&mut game);
    let (mut done, mut stalls) = (None, 0);
    for tick in 1..=120 {
        game.step();
        stalls += game.resource::<StalledWalkers>().get();
        if done.is_none() && game.part::<Destination>(walker).get().is_none() {
            done = Some(tick);
        }
    }
    let at = game.part::<Position>(walker).get();
    assert_eq!(
        (
            done,
            stalls,
            game.part::<ArrivedAt>(walker).get(),
            *game.part::<ArrivedShort>(walker)
        ),
        (Some(26), 0, Some(goal), ArrivedShort::of(None))
    );
    assert_eq!(at.to_bits(), [1632142, 0, 0]);
}

#[test]
fn a_route_goes_round_a_block_of_idle_friends_and_through_it_when_walled_in() {
    // Half-meter cells over ±12 m. A block of 8 idle friends of 0.5 m, at x = ±0.5 and z = ±0.5,
    // ±1.5, holds the cells whose centers lie within the two radii, 1 m, of one, columns from
    // x = −1.25 to 1.25 and rows from z = −2.25 to 2.25, to a walker of 0.5 m from (−6, 0) to
    // (6, 0), a quarter meter a tick. Through the block, its 6 columns cost 6 · 42 = 252 more
    // than the straight 12 m, 240; round it, past z = 2.25, a few cells more: so the route goes
    // round, and the walker arrives in its tick 54, 6 past the 48 the straight line takes.
    // Boxes of 10 by 4 m at (0, ±4) wall the block in, from z = −2 to 2: the route goes through
    // it, row z = 0.25 a waypoint a cell, as a line across a priced cell is not smoothed, and the
    // friends step aside, 2 deep at most, each to a place clear of the walker's way as far as
    // its window reaches, and take its pushes, so the walker arrives in its tick 58
    // (04-capabilities/navigation.md, A route prices the units on it;
    // 04-capabilities/navigation.md, A friend yields to the nearest place clear of the way).
    let half = Num::HALF;
    let goal = Vec3::from((6, 0, 0));
    let run = |walled: bool| {
        let mut game = walk_match();
        Navigation::load_open_pathing(&mut game, half, [-12, -12], [12, 12], [ground(half)]);
        if walled {
            let wall = BodyForm::new(
                BodyShape::box_sized([Num::int(10), Num::int(4)]).unwrap(),
                Layer::FIRST,
            );
            for z in [-4, 4] {
                Navigation::walker_at(&mut game, (0, 0, z))
                    .body(wall)
                    .place(&mut game);
            }
        }
        for (x, z) in [-1, 1]
            .into_iter()
            .flat_map(|x| [-3, -1, 1, 3].map(|z| (x, z)))
        {
            let at = Vec3::new(Num::int(x) / 2, Num::ZERO, Num::int(z) / 2);
            Navigation::walker_at(&mut game, at)
                .circle(half)
                .walks(Num::QUARTER, None)
                .place(&mut game);
        }
        game.step();
        let walker = Navigation::walker_at(&mut game, (-6, 0, 0))
            .circle(half)
            .walks(Num::QUARTER, Some(goal))
            .place(&mut game);
        game.step();
        let route = Navigation::ahead(&game, walker).to_vec();
        let mut done = None;
        for tick in 1..120 {
            game.step();
            if done.is_none() && game.part::<Destination>(walker).get().is_none() {
                done = Some(tick + 1);
            }
        }
        (route, done, game.part::<Position>(walker).get())
    };
    let quarters = |x: i64, z: i64| Vec3::new(Num::int(x) / 4, Num::ZERO, Num::int(z) / 4);
    let round = [
        quarters(-3, 11),
        quarters(3, 11),
        quarters(5, 9),
        quarters(7, 7),
        goal,
    ];
    assert_eq!(run(false), (round.to_vec(), Some(54), goal));
    let through: Vec<Vec3> = (0..8)
        .map(|column| quarters(2 * column - 7, 1))
        .chain([goal])
        .collect();
    assert_eq!(run(true), (through, Some(58), goal));
}

#[test]
fn a_wide_walker_passes_wide_idle_friends_that_yield_to_free_room_and_stops_at_hemmed_ones() {
    // Half-meter cells over ±12 m. A lane along x, z −1.5 to 1.5, walled below by a box from
    // z = −5.5 and above by boxes to z = 5.5, closed behind by one from 5.5 to 7.5; with
    // alcoves, the boxes above leave two dead ends 3 m wide, x −3 to 0 and 2 to 5. Two idle
    // friends of 1 m that drive stand in the lane at (−1.5, 0) and (3.5, 0), and a walker of 1 m
    // that drives goes from (−8, 0) to (9, 0), a quarter meter a tick, its route through them, as
    // the lane is the only way. With alcoves, each friend steps to the nearest cell whose center
    // lies the two radii, 2 m, off the walker's way, up its alcove, at (−1.25, 2.25) and
    // (3.75, 2.25), and the walker arrives in tick 75, 7 past the 68 its 17 m take
    // (04-capabilities/navigation.md, A friend yields to the nearest place clear of the way).
    // With none, the first friend finds no such place and is hemmed in, firm; the walker, stuck
    // against it, asks its route again, which walls the friend's cells and so reaches no
    // farther, and it arrives short of its goal in tick 51, at the reached cell nearest the goal;
    // the second friend is never asked; an order of its own frees the first, which then walks
    // (04-capabilities/navigation.md, A friend that cannot yield is firm, and a stuck walker's
    // route walls it).
    let one = Num::ONE;
    let goal = Vec3::from((9, 0, 0));
    let run = |alcoves: bool| {
        let mut game = walk_match();
        Navigation::load_open_pathing(&mut game, Num::HALF, [-12, -12], [12, 12], [ground(one)]);
        let unit_type = Navigation::walker_type(&mut game);
        let navigation = UnitNavigationData::new(Gait::Drive, Num::ONE).unwrap();
        Navigation::load_unit_navigation(&mut game, unit_type, navigation);
        let box_at = |game: &mut Match, [x0, x1, z0, z1]: [i64; 4]| {
            let size = [Num::int(x1 - x0) / 2, Num::int(z1 - z0) / 2];
            let at = Vec3::new(Num::int(x0 + x1) / 4, Num::ZERO, Num::int(z0 + z1) / 4);
            Navigation::walker_at(game, at)
                .body(BodyForm::new(
                    BodyShape::box_sized(size).unwrap(),
                    Layer::FIRST,
                ))
                .place(game);
        };
        // Each box by its edges in half meters: x from, x to, z from, z to.
        box_at(&mut game, [-24, 24, -11, -3]);
        box_at(&mut game, [-24, 24, 11, 15]);
        if alcoves {
            for [x0, x1] in [[-24, -6], [0, 4], [10, 24]] {
                box_at(&mut game, [x0, x1, 3, 11]);
            }
        } else {
            box_at(&mut game, [-24, 24, 3, 11]);
        }
        let friends = [-3, 7].map(|x| {
            Navigation::walker_at(&mut game, Vec3::new(Num::int(x) / 2, Num::ZERO, Num::ZERO))
                .team(Team::new(0))
                .circle(one)
                .walks(Num::QUARTER, None)
                .place(&mut game)
        });
        game.step();
        let walker = Navigation::walker_at(&mut game, (-8, 0, 0))
            .team(Team::new(0))
            .circle(one)
            .walks(Num::QUARTER, Some(goal))
            .place(&mut game);
        let mut done = None;
        for tick in 1..=300 {
            game.step();
            if done.is_none() && game.part::<Destination>(walker).get().is_none() {
                done = Some(tick);
            }
        }
        let hemmed = friends.map(|friend| game.part::<Hemmed>(friend).get());
        let at = friends.map(|friend| game.part::<Position>(friend).get());
        let short = *game.part::<ArrivedShort>(walker);
        let walker_at = game.part::<Position>(walker).get();
        // An order of its own makes the first friend walk, and frees it.
        *game.part_mut::<Destination>(friends[0]) = Destination::to(Some(Vec3::from((-10, 0, 0))));
        game.step();
        let freed = !game.part::<Hemmed>(friends[0]).get();
        (done, walker_at, short, hemmed, at, freed)
    };
    let quarters = |x: i64, z: i64| Vec3::new(Num::int(x) / 4, Num::ZERO, Num::int(z) / 4);
    assert_eq!(
        run(true),
        (
            Some(75),
            goal,
            ArrivedShort::of(None),
            [false; 2],
            [quarters(-5, 9), quarters(15, 9)],
            true
        )
    );
    let friends = [quarters(-6, 0), quarters(14, 0)];
    assert_eq!(
        run(false),
        (
            Some(53),
            quarters(-15, 1),
            ArrivedShort::of(Some(goal)),
            [true, false],
            friends,
            true
        )
    );
}

#[test]
fn walkers_sent_to_one_point_pack_round_it_and_stop() {
    // Five walkers of 0.5 m of team 0 at x = −6, z = −4, −2, 0, 2 and 4, a quarter meter a tick,
    // all to (4, 0), 10 m off and more, and pushed on by the pushes of the others beside them. The
    // middle one arrives first, in tick 37, at the goal; each other touches one that arrived there
    // nearer the goal than it, so stands, arrived too, in ticks 38 and 39, packed round the goal,
    // none circling it and none pushing the first off it (04-capabilities/navigation.md, A crowded
    // goal ends the walk). No tick leaves a pair overlapping past the slack.
    let half = Num::HALF;
    let goal = Vec3::from((4, 0, 0));
    let mut game = walk_match();
    Navigation::load_open_pathing(&mut game, half, [-12, -12], [12, 12], [ground(half)]);
    let walkers = [-2, -1, 0, 1, 2].map(|z| {
        Navigation::walker_at(&mut game, (-6, 0, z * 2))
            .circle(half)
            .walks(Num::QUARTER, Some(goal))
            .place(&mut game)
    });
    let mut arrived = [None; 5];
    for tick in 1..=120 {
        game.step();
        assert_eq!(game.resource::<UnpartedPairs>().get(), 0, "tick {tick}");
        for (at, &id) in walkers.iter().enumerate() {
            if arrived[at].is_none() && game.part::<Destination>(id).get().is_none() {
                arrived[at] = Some(tick);
            }
        }
    }
    assert_eq!(arrived, [39, 38, 37, 38, 39].map(Some));
    assert_eq!(
        walkers.map(|id| game.part::<ArrivedAt>(id).get()),
        [Some(goal); 5]
    );
    // In bits of 2⁻²⁴: the first at (4.033, 0.002), pushed a little past the goal by the others'
    // last pushes, the next two either side behind it at x 3.03 and 3.05, the last two behind
    // those at x 2.20 and 2.10.
    assert_eq!(
        walkers.map(|id| game.part::<Position>(id).get().to_bits()),
        [
            [36834922, 0, -17461159],
            [50918212, 0, -8343268],
            [67666917, 0, 38777],
            [51196187, 0, 8431646],
            [35301607, 0, 13801685],
        ]
    );
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
        let mut game = walk_match();
        if planned {
            Navigation::load_open_pathing(&mut game, half, [-8, -8], [8, 8], [ground(half)]);
        }
        let east = Navigation::walker_at(&mut game, (-4, 0, 0))
            .circle(half)
            .walks(quarter, Some(Vec3::from((4, 0, 0))))
            .place(&mut game);
        let west = Navigation::walker_at(&mut game, (4, 0, 0))
            .circle(half)
            .walks(quarter, Some(Vec3::from((-4, 0, 0))))
            .place(&mut game);
        let mut sides = [Num::ZERO; 2];
        let mut arrived = [None; 2];
        for tick in 0..120 {
            game.step();
            for (side, id) in sides.iter_mut().zip([east, west]) {
                let z = game.part::<Position>(id).z;
                if z.to_bits().abs() > side.to_bits().abs() {
                    *side = z;
                }
            }
            for (at, id) in arrived.iter_mut().zip([east, west]) {
                if at.is_none() && game.part::<Destination>(id).get().is_none() {
                    *at = Some(tick);
                }
            }
        }
        if planned {
            // Seven ticks more each than the 32 a straight 8 m takes, round the other.
            assert_eq!(arrived, [Some(38); 2]);
            assert_eq!(game.part::<Position>(east).get(), Vec3::from((4, 0, 0)));
            assert_eq!(game.part::<Position>(west).get(), Vec3::from((-4, 0, 0)));
            // As they pass, each goes about half of the two radii, 1 m, off the line, on its own
            // side: in bits, 0.49263 m on −z and 0.49643 m on +z, as each counts stuck by the
            // waypoints of its own route, which the cells' order of search makes no mirror of the
            // other's (walkers-get-through.md, A walker progresses only when it comes nearer its
            // waypoint).
            assert_eq!(sides.map(Num::to_bits), [-8264983, 8328618]);
        } else {
            // Pressed together, each half a metre from where they met, at x = 0.
            assert_eq!(arrived, [None; 2]);
            let half_x = |x: Num| Vec3::new(x, Num::ZERO, Num::ZERO);
            let pressed = [east, west].map(|id| game.part::<Position>(id).get());
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
    let mut game = walk_match();
    let flyer = Walker {
        layer: AIR,
        radius: half,
    };
    Navigation::load_open_pathing(
        &mut game,
        Num::ONE,
        [0, 0],
        [12, 6],
        vec![ground(half), flyer],
    );
    for z in [1, 3, 5] {
        Navigation::walker_at(&mut game, (6, 0, z))
            .circle(Num::ONE)
            .place(&mut game);
    }
    let standing = Navigation::walker_at(&mut game, (3, 0, 3))
        .circle(half)
        .walks(Num::ONE, None)
        .place(&mut game);
    // An air unit of 0.5 m from (1, 3) to (11, 3), a meter a tick, flies straight over both: in
    // tick 2 it is on the ground unit, which does not move, and in tick 10 at its goal.
    let body = BodyForm::new(BodyShape::circle(half).unwrap(), AIR);
    let flying = Navigation::walker_at(&mut game, (1, 0, 3))
        .body(body)
        .walks(Num::ONE, Some(Vec3::from((11, 0, 3))))
        .place(&mut game);
    // A ground unit of 0.5 m from (1, 1) to (11, 1) stops short of the wall, at the cell center
    // nearest its goal that it reaches: (4.5, 0.5) and (4.5, 1.5) tie at √42.5 ≈ 6.52 m, each
    // √2.5 ≈ 1.58 m from the tower at (6, 1), and the lower cell number wins. It walks straight
    // there, √12.5 ≈ 3.54 m, so it arrives in tick 4.
    let walking = Navigation::walker_at(&mut game, (1, 0, 1))
        .circle(half)
        .walks(Num::ONE, Some(Vec3::from((11, 0, 1))))
        .place(&mut game);
    let short = Vec3::new(Num::int(4) + half, Num::ZERO, half);
    let mut flown = Vec::new();
    for _ in 0..12 {
        game.step();
        flown.push(game.part::<Position>(flying).get());
        assert_eq!(game.part::<Position>(standing).get(), Vec3::from((3, 0, 3)));
    }
    let line: Vec<Vec3> = (2..=11)
        .chain([11; 2])
        .map(|x| Vec3::from((x, 0, 3)))
        .collect();
    assert_eq!(flown, line);
    assert_eq!(game.part::<Position>(walking).get(), short);
}

#[test]
fn routes_wait_past_the_limit_of_work_in_the_order_asked() {
    // Two rows of 16 cells of 1 m, a post on cell (8, 0). A route along row 0 to x = 3.5 sees
    // its goal, and is the line: the test of the goal and of the line, 2 (walkers-get-through.md,
    // A route in sight is a line). One past the post searches round it, by (9.5, 1.5): to the
    // far end, the goal's test, the line's, 17 cells expanded and the smoothing's 14 lines, 33;
    // to x = 14.5, 1 + 1 + 16 + 13 = 31. A tick does up to the grid's 32.
    let mut game = walk_match();
    Navigation::load_open_pathing(
        &mut game,
        Num::ONE,
        [0, 0],
        [16, 2],
        vec![ground(Num::ZERO)],
    );
    let half = Num::HALF;
    let post = BodyForm::new(
        BodyShape::box_sized([Num::ONE, Num::ONE]).unwrap(),
        Layer::FIRST,
    );
    Navigation::walker_at(&mut game, Vec3::new(Num::int(8) + half, Num::ZERO, half))
        .body(post)
        .place(&mut game);
    let place = |x: i64| Vec3::new(Num::int(x) + half, Num::ZERO, half);
    let (start, far, near) = (place(0), place(15), place(3));
    let units = [None, Some(far), Some(near), Some(far)]
        .map(|goal| Navigation::unit(&mut game, start, goal));
    let waiting = |game: &Match| {
        units.map(|id| {
            let entity = game.unit_entity(id);
            game.get::<Route>(entity).unwrap().asked().is_some()
        })
    };
    // The last three ask in tick 0; the second does 33 and meets the limit.
    game.step();
    assert_eq!(waiting(&game), [false, false, true, true]);
    let round = Vec3::new(Num::int(9) + half, Num::ZERO, Num::ONE + half);
    assert_eq!(Navigation::ahead(&game, units[1]), [round, far]);
    // The first, of the lowest id, asks in tick 1, after the rest. The last asks again in tick
    // 1, for x = 14.5, and keeps its place of tick 0. So the third and the last go first, 2 and
    // 31, which meets the limit, and the first waits; had the last's ask moved to tick 1, the
    // first and the third would go before it, 2 each, and none would wait.
    let first = game.unit_entity(units[0]);
    let mut destination = game.get_mut::<Destination>(first).unwrap();
    *destination = Destination::to(Some(near));
    let last = game.unit_entity(units[3]);
    let mut destination = game.get_mut::<Destination>(last).unwrap();
    *destination = Destination::to(Some(place(14)));
    game.step();
    assert_eq!(waiting(&game), [true, false, false, false]);
    game.step();
    assert_eq!(waiting(&game), [false; 4]);
}

#[test]
fn a_walker_steers_with_the_work_the_routes_left() {
    // A walker of 0.25 m from (0.5, 1.5) to (11.5, 1.5) on 12 by 3 cells of 1 m, a limit of 36,
    // and a post on cell (6, 0), 0.5 m off the walker's line: its route in tick 0 is the line it
    // sees, 2 (walkers-get-through.md, A route in sight is a line). After it, a unit of 0.25 m
    // stands on the line at (3.5, 1.5), and walkers with no body ask for routes along z = 0.5,
    // which the post blocks: each searches round it, expanding 13 cells, and with a unit that
    // stands each line test visits the cells it touches, as it tests the crowd's with the walls',
    // so each asker's route costs 70, past the limit, and spends its tick. With none, the walker
    // steers round the unit in tick 1; with one, in tick 2; with two, in tick 3. The mode moves no
    // unit, so the unit stays.
    let quarter = Num::QUARTER;
    let half = Num::HALF;
    let place = |x: i64, z: i64| Vec3::new(Num::int(x) + half, Num::ZERO, Num::int(z) + half);
    let goal = place(11, 1);
    for (askers, steered) in [
        (0, [true; 3]),
        (1, [false, true, true]),
        (2, [false, false, true]),
    ] {
        let mut game = walk_match();
        Navigation::load_moves(&mut game, Moves::None);
        Navigation::load_open_pathing(
            &mut game,
            Num::ONE,
            [0, 0],
            [12, 3],
            vec![ground(Num::ZERO), ground(quarter)],
        );
        let post = BodyForm::new(
            BodyShape::box_sized([Num::ONE, Num::ONE]).unwrap(),
            Layer::FIRST,
        );
        Navigation::walker_at(&mut game, place(6, 0))
            .body(post)
            .place(&mut game);
        let walker = Navigation::walker_at(&mut game, place(0, 1))
            .circle(quarter)
            .walks(quarter, Some(goal))
            .place(&mut game);
        game.step();
        assert_eq!(Navigation::ahead(&game, walker), [goal]);
        Navigation::walker_at(&mut game, place(3, 1))
            .circle(quarter)
            .walks(quarter, None)
            .place(&mut game);
        for _ in 0..askers {
            Navigation::unit(&mut game, place(0, 0), Some(place(11, 0)));
        }
        let ticks = [(); 3].map(|()| {
            game.step();
            Navigation::ahead(&game, walker) != [goal]
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
    let place = |x: i64, z: i64| Vec3::new(Num::int(x) + half, Num::ZERO, Num::int(z) + half);
    let mut game = walk_match();
    Navigation::load_open_pathing(&mut game, Num::ONE, [0, 0], [8, 3], [ground(quarter)]);
    let goal = place(7, 1);
    let walker = Navigation::walker_at(&mut game, place(0, 1))
        .circle(quarter)
        .walks(Num::ONE, Some(goal))
        .place(&mut game);
    game.step();
    let entity = game.unit_entity(walker);
    let changed = |game: &Match| {
        game.entity(entity)
            .get_ref::<Route>()
            .unwrap()
            .last_changed()
    };
    let planned = changed(&game);
    Navigation::walker_at(&mut game, place(5, 0))
        .circle(half)
        .place(&mut game);
    game.step();
    assert_eq!(changed(&game), planned);
    assert_eq!(Navigation::ahead(&game, walker), [goal]);
    let tower = Navigation::walker_at(&mut game, place(5, 1))
        .circle(half)
        .place(&mut game);
    game.step();
    assert_ne!(Navigation::ahead(&game, walker), [goal]);
    assert!(game.part::<Route>(walker).is_reached());

    // Pressed against that tower, 0.7 m from its center, closer than the two radii, on a route
    // straight through it, as no change of the static bodies asks again: a match restored there
    // plays as the match does. Its new index's first fill puts no body in, so no route is asked
    // again for the towers that stood there before.
    let pressed = Vec3::new(Num::int(48) / 10, Num::ZERO, Num::int(1) + half);
    let through = PlannedRoute::new(goal, pressed, &[goal]);
    game.insert_parts(walker, (Position::from(pressed), through.parts()));
    let mut save = Vec::new();
    game.snapshot(&mut save);
    let mut restored = walk_match();
    Navigation::load_open_pathing(&mut restored, Num::ONE, [0, 0], [8, 3], [ground(quarter)]);
    Release::restore_into(&game, &mut restored);
    let mut played = Vec::new();
    for _ in 0..3 {
        game.step();
        restored.step();
        assert_eq!(restored.part::<Route>(walker), game.part::<Route>(walker));
        assert_eq!(restored.state_hash(), game.state_hash());
        played.push(game.state_hash());
    }

    // Loaded in place there once that tower went, it plays those ticks again: its index clears
    // and fills first again, so the tower the load puts back asks no route again.
    let gone = restored.unit_entity(tower);
    restored.despawn(gone);
    restored.step();
    Release::clear(&mut restored);
    restored.restore(&save).unwrap();
    for hash in played {
        restored.step();
        assert_eq!(restored.state_hash(), hash);
    }
}

#[test]
fn a_walker_that_arrives_short_waits_there_until_a_static_body_goes() {
    // Towers of 0.5 m down column 4 of 8 by 3 cells of 1 m wall off a walker of 0.25 m at (0.5,
    // 1.5) from its goal, (6.5, 1.5): each blocks its own cell alone, whose center is on it, and
    // not its neighbors', 1 m off, past the two radii, 0.75 m. Its route ends at the nearest
    // cell it reaches, (3.5, 1.5), 3 m on, which it arrives at in tick 2, a meter a tick.
    let quarter = Num::QUARTER;
    let half = Num::HALF;
    let place = |x: i64, z: i64| Vec3::new(Num::int(x) + half, Num::ZERO, Num::int(z) + half);
    let goal = place(6, 1);
    let walled = || {
        let mut game = walk_match();
        Navigation::load_open_pathing(&mut game, Num::ONE, [0, 0], [8, 3], [ground(quarter)]);
        let towers = [0, 1, 2].map(|z| {
            Navigation::walker_at(&mut game, place(4, z))
                .circle(half)
                .place(&mut game)
        });
        let walker = Navigation::walker_at(&mut game, place(0, 1))
            .circle(quarter)
            .walks(Num::ONE, Some(goal))
            .place(&mut game);
        let middle = game.unit_entity(towers[1]);
        (game, middle, walker)
    };
    // The middle tower dies while the walker is on its way, at (1.5, 1.5) after tick 0: its route
    // ends short, so it asks again in tick 1, and walks on straight to the goal, 6 m in 6 ticks.
    let (mut game, middle, walker) = walled();
    game.step();
    game.entity_mut(middle).insert(Dead);
    game.step();
    assert!(game.part::<Route>(walker).is_reached());
    for _ in 0..4 {
        game.step();
    }
    assert_eq!(game.part::<Position>(walker).get(), goal);
    // The middle tower stands, and the walker arrives short.
    let (mut game, middle, walker) = walled();
    for _ in 0..3 {
        game.step();
    }
    let short = place(3, 1);
    assert_eq!(game.part::<Position>(walker).get(), short);
    assert_eq!(game.part::<Destination>(walker).get(), None);
    assert!(game.part::<ArrivedShort>(walker).of_goal(goal));
    // Sent there again, as an order would, it plans nothing, and stays with no destination.
    let entity = game.unit_entity(walker);
    let changed = |game: &Match| {
        game.entity(entity)
            .get_ref::<Route>()
            .unwrap()
            .last_changed()
    };
    let before = changed(&game);
    *game.get_mut::<Destination>(entity).unwrap() = Destination::to(Some(goal));
    game.step();
    assert_eq!(changed(&game), before);
    assert_eq!(game.part::<Destination>(walker).get(), None);
    assert_eq!(game.part::<Position>(walker).get(), short);
    // The middle tower dies, and the walker goes on through its cell from the next tick: 3 m to
    // the gap, (4.5, 1.5), then 2 m on, in 3 ticks. Its route reached the goal, so it forgets the
    // route, and its progress.
    game.entity_mut(middle).insert(Dead);
    game.step();
    assert_eq!(game.part::<Destination>(walker).get(), Some(goal));
    for _ in 0..2 {
        game.step();
    }
    assert_eq!(game.part::<Position>(walker).get(), goal);
    assert_eq!(game.part::<Destination>(walker).get(), None);
    assert_eq!(game.part::<Route>(walker).goal(), None);
    assert_eq!(*game.part::<Progress>(walker), Progress::default());
}

#[test]
fn a_dead_unit_forgets_its_destination_and_a_stopped_one_keeps_it() {
    let mut game = blocking_walk_match();
    let unit = Navigation::unit(&mut game, (0, 0, 0), Some(Vec3::from((0, 0, 5))));
    game.insert_parts(unit, Dead);
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
            let id = Navigation::unit(&mut game, (x, 0, 0), Some(Vec3::from((x, 0, 5))));
            Units::set_blocks(&mut game, id, blocks);
            (id, x)
        })
        .collect();
    game.step();
    assert_eq!(game.part::<Position>(unit).get(), Vec3::from((0, 0, 0)));
    assert_eq!(game.part::<Destination>(unit).get(), None);
    for (&(id, x), (blocks, walks)) in walkers.iter().zip(cases) {
        let z = i16::from(walks);
        assert_eq!(
            game.part::<Position>(id).get(),
            Vec3::from((x, 0, z)),
            "{blocks:?}"
        );
        assert_eq!(
            game.part::<Destination>(id).get(),
            Some(Vec3::from((x, 0, 5))),
            "{blocks:?}"
        );
        Units::set_blocks(&mut game, id, &[]);
    }
    // The blocks end: each walks on a meter from where it stood.
    game.step();
    for (&(id, x), (blocks, walks)) in walkers.iter().zip(cases) {
        let z = i16::from(walks) + 1;
        assert_eq!(
            game.part::<Position>(id).get(),
            Vec3::from((x, 0, z)),
            "{blocks:?}"
        );
    }
}

#[test]
fn every_navigation_type_is_state() {
    let mut game = walk_match();
    let id = Navigation::unit(&mut game, (0, 0, 0), Some(Vec3::from((0, 0, 5))));
    game.insert_parts(
        id,
        (
            PathWalker::start(PathEnd::Start, id),
            OnPath::new(PathId::new(0)),
        ),
    );
    let ask = RouteAsk {
        goal: Vec3::from((3, 0, 4)),
        tick: Tick::new(2),
        party: None,
    };
    Navigation::ask_route(&mut game, id, ask);
    let lane = || Paths::new([("lane", &[Vec3::from((0, 0, 0)), Vec3::from((0, 0, 5))][..])]);
    Navigation::load_paths(&mut game, lane());
    // A restore loads the map first, as the packages give it.
    let mut restored = walk_match();
    Navigation::load_paths(&mut restored, lane());
    Release::restore_into(&game, &mut restored);
}

mod boxes;
mod forced;
mod groups;
