//! A walker's route: round the static bodies and the units, within the tick's work, asked again
//! as the bodies change (04-capabilities/navigation.md).

use super::*;

#[test]
fn the_pathing_grid_takes_a_body_as_it_spawns_and_its_changes_from_the_next_tick() {
    // 1 m cells over (−2, −2) to (2, 2), for walkers of 0.5 m: a tower of 0.5 m at (−1.5, −1.5)
    // blocks its own cell, 0, whose center is on it; one the client only holds, at (1.5, 1.5),
    // blocks cell 15. Each blocks as it spawns, as navigation's spawn callback gives the grid
    // the body (04-capabilities/navigation.md, A spawn stands where it can walk). A walker never
    // marks the grid.
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

#[test]
fn a_walker_reaches_a_goal_by_a_unit_that_stands_and_ends_short_of_one_inside_it() {
    // A walker of 0.5 m of team 0 walks from (−4, 0) a quarter meter a tick, and a friend of 4 m
    // at work stands at (5, 0). A goal at (0.4, 0) lies 4.6 m off the friend, past their two
    // radii, 4.5 m: with no push between them, the walker stands on it in tick 18, 4.4 m on
    // (entries-and-blocking.md, No personal space between walkers). A goal at (0.6, 0) lies within
    // the two radii, where the walker's body cannot stand: it brakes to touch the friend, and its
    // short route round the friend ends on the reached cell nearest the goal, (0.25, −0.25), where
    // it arrives short in tick 19 (04-capabilities/navigation.md, A route ends short, not never).
    let half = Num::HALF;
    let run = |goal: Vec3| {
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
        for tick in 1..=60 {
            game.step();
            stalls += game.resource::<StalledWalkers>().get();
            if done.is_none() && game.part::<Destination>(walker).get().is_none() {
                done = Some(tick);
            }
        }
        let at = game.part::<Position>(walker).get();
        (done, stalls, at, *game.part::<ArrivedShort>(walker))
    };
    let near = Vec3::new(Num::int(2) / 5, Num::ZERO, Num::ZERO);
    assert_eq!(run(near), (Some(18), 0, near, ArrivedShort::of(None)));
    let inside = Vec3::new(Num::int(3) / 5, Num::ZERO, Num::ZERO);
    let cell = Vec3::new(Num::QUARTER, Num::ZERO, -Num::QUARTER);
    assert_eq!(
        run(inside),
        (Some(19), 0, cell, ArrivedShort::of(Some(inside)))
    );
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
    // friends step aside, each to a place clear of the walker's way that it reaches without
    // passing the walker; the walker and each friend that steps aside walk through the idle
    // friends they move aside, which step aside in turn, braking behind each until it clears,
    // and the walker arrives in its tick 68 (04-capabilities/navigation.md, A route prices the
    // units on it; entries-and-blocking.md, A walker brakes behind a unit that blocks it;
    // 04-capabilities/navigation.md, A friend yields to the nearest place clear of the way;
    // 04-capabilities/navigation.md, A walker walks through the friends it moves aside).
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
    assert_eq!(run(true), (through, Some(68), goal));
}

#[test]
fn routes_wait_past_the_limit_of_work_in_the_order_asked() {
    // Two rows of 16 cells of 1 m, a post on cell (8, 0). A route along row 0 to x = 3.5 sees
    // its goal, and is the line: the test of the goal and of the line, 2
    // (04-capabilities/navigation.md, A route in sight is a line). One past the post searches round
    // it, by (9.5, 1.5): to the far end, the goal's test, the line's, 17 cells expanded and the
    // smoothing's 14 lines, 33; to x = 14.5, 1 + 1 + 16 + 13 = 31. A tick does up to the grid's 32.
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
fn a_scan_in_act_counts_against_the_routes_of_its_tick() {
    // 16 by 2 cells of 1 m, a limit of 32. In tick 0 a system in Act scans 32 times for the open
    // cell nearest a point in an open cell, each the test of that cell, 1, so the scans spend the
    // tick's limit before Move, and the route a unit asks in that tick waits for tick 1, where it
    // goes (04-capabilities/navigation.md, One work limit for every route).
    let mut game = walk_match();
    Navigation::load_open_pathing(
        &mut game,
        Num::ONE,
        [0, 0],
        [16, 2],
        vec![ground(Num::ZERO)],
    );
    let place = |x: i64| Vec3::new(Num::int(x) + Num::HALF, Num::ZERO, Num::HALF);
    let unit = Navigation::unit(&mut game, place(0), Some(place(3)));
    let scanned = place(5);
    game.add_tick_systems(
        (move |world: &mut World| {
            if world.resource::<SimTick>().start() == Tick::ZERO {
                for _ in 0..32 {
                    Navigation::open_cell(world, ground(Num::ZERO), scanned).unwrap();
                }
            }
        })
        .in_set(SimSystems::Act)
        .after(NavigationSystems::FollowPaths),
    );
    let asked = |game: &Match| {
        let entity = game.unit_entity(unit);
        game.get::<Route>(entity).unwrap().asked().is_some()
    };
    game.step();
    assert!(asked(&game));
    game.step();
    assert!(!asked(&game));
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
    game.steps(4);
    assert_eq!(game.part::<Position>(walker).get(), goal);
    // The middle tower stands, and the walker arrives short.
    let (mut game, middle, walker) = walled();
    game.steps(3);
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
    game.steps(2);
    assert_eq!(game.part::<Position>(walker).get(), goal);
    assert_eq!(game.part::<Destination>(walker).get(), None);
    assert_eq!(game.part::<Route>(walker).goal(), None);
    assert_eq!(*game.part::<Progress>(walker), Progress::default());
}
