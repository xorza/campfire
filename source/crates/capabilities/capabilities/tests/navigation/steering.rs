//! A walker on its way: it walks, turns and steers round the units that stand, passes the
//! friends it walks into, and stops (04-capabilities/navigation.md).

use super::*;

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
    // stands on from that stretch, and the walker, braked a little behind it as it steps, is done
    // in tick 60 (04-capabilities/navigation.md, An idle friend steps aside). Units that drive do
    // the same: the walker is done in tick 60 too (04-capabilities/navigation.md, A unit that drives
    // yields by driving). An enemy stays, and so does a friend at work. The walker the
    // enemy holds is stuck once, and the route it asks walls the enemy's cells and goes round the
    // corridor's walls: one stall, and it arrives in tick 202 (04-capabilities/navigation.md, A
    // stuck walker's route walls the units of other teams that stand). The one the friend at work
    // holds never arrives: it comes no nearer its waypoint from tick 32, and, stuck 30 ticks, 1 s,
    // it asks its route again in tick 62, answered as the next tick starts, whose waypoint it takes
    // fresh, and again each 31 ticks, whatever detour it takes (04-capabilities/navigation.md, A
    // walker progresses only when it comes nearer its waypoint); the sixth time, past its 5 asks,
    // it stops where it stands, in tick 62 + 5 · 31 = 217, with no destination and no goal to
    // arrive short of (04-capabilities/navigation.md, A stuck walker plans again, then stops): 6
    // stalls, which navigation's report counts, against none of a walker the friend steps aside
    // for.
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
    // Where the friend ends, in bits of 2⁻²⁴: on its place aside, the two radii off the walker's
    // way, at (0.5, −0.75), as no walker pushes it (entries-and-blocking.md, A unit outside the
    // cone neither blocks nor pushes).
    let ends = [
        (Gait::Step, 60, [8388608, 0, -12582912]),
        (Gait::Drive, 60, [8388608, 0, -12582912]),
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
    // goes round the friend's cells, along z = 0.25, and the walker brakes to touch the friend
    // there, at x = 0.5 − √0.9375, its body's 1 m from the friend's, which stays where it stood,
    // as no overlap is left to part (entries-and-blocking.md, A walker brakes behind a unit that
    // blocks it).
    let held = [
        (1, false, [[100663296, 0, 0], [8388608, 0, 0]], 202, 1),
        (0, true, [[-7855862, 0, 4194304], [8388608, 0, 0]], 217, 6),
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
fn a_walker_passes_through_the_idle_friend_it_walks_into_and_a_client_walks_it_alike() {
    // A walker of 0.5 m at (−4, 0) walks into an idle friend of 0.5 m at (2, 0), a meter a tick,
    // on open ground: the friend is on its way, but its walk goes into it, so it does not step
    // aside, its cells cost the walker's route nothing and collision does not part the two; the
    // walker reaches the friend's place in tick 5, 6 m on, and has arrived, its walk over, so
    // collision parts the two on one spot along x, the walker, of the higher id, to +x, half a
    // meter each way. A client predicts the walk alike, as it moves no friend
    // aside (entries-and-blocking.md, A walker passes through the unit it walks into).
    let half = Num::HALF;
    let mut game = walk_match();
    let mut client = Match::builder(&[Capability::Navigation])
        .role(Role::Client)
        .build();
    Navigation::walker_type(&mut client);
    let mut walker = None;
    let mut friend = None;
    for game in [&mut game, &mut client] {
        Navigation::load_open_pathing(game, half, [-8, -8], [8, 8], [ground(half)]);
        friend = Some(
            Navigation::walker_at(game, (2, 0, 0))
                .team(Team::new(0))
                .circle(half)
                .walks(Num::ONE, None)
                .place(game),
        );
        walker = Some(
            Navigation::walker_at(game, (-4, 0, 0))
                .team(Team::new(0))
                .circle(half)
                .walks(Num::ONE, None)
                .place(game),
        );
    }
    let (walker, friend) = (walker.unwrap(), friend.unwrap());
    let into = Destination::to_unit(friend, Vec3::from((2, 0, 0)));
    for game in [&mut game, &mut client] {
        *game.part_mut::<Destination>(walker) = into;
    }
    let mut arrived = None;
    for tick in 0..8 {
        game.step();
        client.step();
        if tick < 5 {
            let still = game.part::<Position>(friend).get();
            assert_eq!(still, Vec3::from((2, 0, 0)), "tick {tick}");
        }
        assert_eq!(
            client.part::<Position>(walker).get(),
            game.part::<Position>(walker).get(),
            "tick {tick}"
        );
        if arrived.is_none() && game.part::<Destination>(walker).get().is_none() {
            arrived = Some((tick, game.part::<Position>(walker).get()));
        }
    }
    assert_eq!(arrived, Some((5, halves(5, 0))));
    assert_eq!(game.part::<Position>(friend).get(), halves(3, 0));
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
    // (3.75, 2.25), and the walker, braked behind each until it clears its way, arrives in tick
    // 79, 11 past the 68 its 17 m take (04-capabilities/navigation.md, A friend yields to the
    // nearest place clear of the way; entries-and-blocking.md, A walker brakes behind a unit that
    // blocks it).
    // With none, a friend's search passes through the other to a place ahead, past the walker's
    // way, and the other steps ahead in turn (04-capabilities/navigation.md, A friend yields
    // through the friends that yield). With the lane open past x = 6, they leave it, the first to
    // (7.25, 2.25), the second, parted from the walker as it passes, to (7.24, −1.87), and the
    // walker arrives in tick 81.
    // With it closed, they drive on until the lane's end holds no place clear of the walker's way,
    // at x 7.25 and 9.25, each hemmed in, firm, the first as its step aside ends short where it
    // began; the walker's route walls them, and it arrives short of its goal in tick 64, at
    // (5.25, −0.25), the reached cell nearest the goal; an order of its own frees the first, which
    // then walks (04-capabilities/navigation.md, A friend that cannot yield is firm, and a stuck
    // walker's route walls it).
    let one = Num::ONE;
    let goal = Vec3::from((9, 0, 0));
    let run = |lane: Lane| {
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
        let end = if lane == Lane::Open { 12 } else { 24 };
        box_at(&mut game, [-24, end, -11, -3]);
        box_at(&mut game, [-24, end, 11, 15]);
        if lane == Lane::Alcoves {
            for [x0, x1] in [[-24, -6], [0, 4], [10, 24]] {
                box_at(&mut game, [x0, x1, 3, 11]);
            }
        } else {
            box_at(&mut game, [-24, end, 3, 11]);
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
        let rate = *game.resource::<MatchRate>();
        let hemmed_now = |game: &Match, friend| game.part::<Hemmed>(friend).holds(game.now(), rate);
        let mut done = None;
        let mut hemmed = [false; 2];
        for tick in 1..=300 {
            game.step();
            if done.is_none() && game.part::<Destination>(walker).get().is_none() {
                done = Some(tick);
                hemmed = friends.map(|friend| hemmed_now(&game, friend));
            }
        }
        // A mark holds while a walker that met it may still ask routes, 5.15 s, 155 ticks at 30
        // a second, so none holds 300 ticks on.
        assert_eq!(friends.map(|friend| hemmed_now(&game, friend)), [false; 2]);
        let at = friends.map(|friend| game.part::<Position>(friend).get().to_bits());
        let short = *game.part::<ArrivedShort>(walker);
        let walker_at = game.part::<Position>(walker).get();
        // An order of its own makes the first friend walk, and frees it.
        *game.part_mut::<Destination>(friends[0]) = Destination::to(Some(Vec3::from((-10, 0, 0))));
        game.step();
        let freed = *game.part::<Hemmed>(friends[0]) == Hemmed::default();
        (done, walker_at, short, hemmed, at, freed)
    };
    let quarters = |x: i64, z: i64| Vec3::new(Num::int(x) / 4, Num::ZERO, Num::int(z) / 4);
    let bits = |x: i64, z: i64| quarters(x, z).to_bits();
    assert_eq!(
        run(Lane::Alcoves),
        (
            Some(79),
            goal,
            ArrivedShort::of(None),
            [false; 2],
            [bits(-5, 9), bits(15, 9)],
            true
        )
    );
    assert_eq!(
        run(Lane::Closed),
        (
            Some(64),
            quarters(21, -1),
            ArrivedShort::of(Some(goal)),
            [true; 2],
            [bits(29, 1), bits(37, 1)],
            true
        )
    );
    assert_eq!(
        run(Lane::Open),
        (
            Some(81),
            goal,
            ArrivedShort::of(None),
            [false; 2],
            [bits(29, 9), [121479201, 0, -31383119]],
            true
        )
    );
}

#[test]
fn walkers_sent_to_one_point_pack_round_it_and_stop() {
    // Five walkers of 0.5 m of team 0 at x = −6, z = −4, −2, 0, 2 and 4, a quarter meter a tick,
    // all to (4, 0), 10 m off and more. The middle one arrives first, in tick 39, on the goal,
    // 10 m on, a tick early as the walkers that close in beside it graze it on as they part; each
    // other touches one that arrived there nearer the goal than it, so stands, arrived too, in
    // ticks 40 and 41, packed round the goal, none circling it and none pushing the first off it
    // (04-capabilities/navigation.md, A crowded goal ends the walk). No tick leaves a pair
    // overlapping past the slack.
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
    assert_eq!(arrived, [41, 40, 39, 40, 41].map(Some));
    assert_eq!(
        walkers.map(|id| game.part::<ArrivedAt>(id).get()),
        [Some(goal); 5]
    );
    // In bits of 2⁻²⁴: the first on the goal, the next two either side behind it at x 3.08 and
    // 3.12, the last two behind those at x 2.21 and 2.24, each walking into the crowd at the goal
    // until it touches one (04-capabilities/navigation.md, A walker walks through the friends it
    // moves aside).
    assert_eq!(
        walkers.map(|id| game.part::<Position>(id).get().to_bits()),
        [
            [37080547, 0, -16660350],
            [51743281, 0, -8506871],
            [67108864, 0, 0],
            [52336016, 0, 8259872],
            [37617503, 0, 16312225],
        ]
    );
}

#[test]
fn a_walker_steers_with_the_work_the_routes_left() {
    // A walker of 0.25 m from (0.5, 1.5) to (11.5, 1.5) on 12 by 3 cells of 1 m, a limit of 36,
    // and a post on cell (6, 0), 0.5 m off the walker's line: its route in tick 0 is the line it
    // sees, 2 (04-capabilities/navigation.md, A route in sight is a line). After it, a unit of
    // 0.25 m stands on the line at (3.5, 1.5), and walkers with no body ask for routes along
    // z = 0.5, which the post blocks: each searches round it, expanding 13 cells, and with a unit
    // that stands each line test visits the cells it touches, as it tests the crowd's with the
    // walls', so each asker's route costs 70, past the limit, and spends its tick. With none, the
    // walker steers round the unit in tick 1; with one, in tick 2; with two, in tick 3. The mode
    // moves no unit, so the unit stays.
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
