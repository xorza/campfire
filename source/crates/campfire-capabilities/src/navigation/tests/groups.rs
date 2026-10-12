use campfire_common::PlayerSlot;

use super::*;
use crate::navigation::party::{Party, PartyKey};

/// Over 1 m cells from (0, 0) to (20, 12), a wall from (4, 0) to (6, 4), as in
/// `a_walker_goes_round_a_wall_smoothed_past_its_corners_and_never_through_it`, and `more` walls,
/// for walkers of no body and, on the air layer, of a quarter meter, which no wall blocks. Its
/// 240 cells let a tick's work plan a party of a few.
fn walled(more: &[([i64; 2], [i64; 2])]) -> Match {
    walled_to([20, 12], more)
}

/// The walled scene up to `max`.
fn walled_to(max: [i64; 2], more: &[([i64; 2], [i64; 2])]) -> Match {
    let box_of = |[x0, z0]: [i64; 2], [x1, z1]: [i64; 2]| {
        let area = [[x0, z0], [x1, z0], [x1, z1], [x0, z1]].map(|point| point.map(Num::int));
        Polygon::new(area.to_vec()).unwrap()
    };
    let walls: Vec<Wall> = [([4, 0], [6, 4])]
        .iter()
        .chain(more)
        .map(|&(low, high)| Wall {
            layer: Layer::FIRST,
            area: box_of(low, high),
        })
        .collect();
    let mut game = walk_match();
    let air = Walker {
        layer: AIR,
        radius: Num::QUARTER,
    };
    game.load_walled(Num::ONE, [0, 0], max, vec![ground(Num::ZERO), air], &walls);
    game
}

/// The route of a walker of no body from (2, 1) to (8, 1) round the wall: over its top, row 4,
/// by (3.5, 4.5) and (6.5, 4.5).
fn over_the_wall() -> [Position; 3] {
    [point(7, 9), point(13, 9), point(16, 2)]
}

/// The point at `x` and `z` half meters.
fn point(x: i64, z: i64) -> Position {
    Position::new(Vec3::new(Num::HALF * x, Num::ZERO, Num::HALF * z)).unwrap()
}

/// The party of an order sent first in tick 0 by player 0, to `goal`.
fn ordered(goal: Position) -> Party {
    Party {
        key: PartyKey::Order {
            tick: Tick::new(0),
            slot: PlayerSlot::new(0),
            number: 0,
        },
        goal,
    }
}

impl Match {
    /// A walker of no body at `pos` that asks in tick 0 for its route to `goal` as one of `party`.
    fn member(&mut self, pos: Position, goal: Position, party: Party) -> StableId {
        let id = self.unit(pos, Some(goal));
        self.part_mut::<Route>(id)
            .ask(goal, Tick::new(0), Some(party));
        id
    }

    fn searches(&self) -> u64 {
        self.resource::<RoutePlanner>().searches()
    }
}

#[test]
fn a_party_searches_once_and_each_member_takes_the_route_from_the_last_waypoint_it_sees() {
    // A move away from the box of (1, 1), (2, 1) and (3, 1), centre (2, 1), to (8, 1): their
    // goals are (7, 1), (8, 1) and (9, 1). The route is searched from (2, 1), the nearest the
    // centre, to (8, 1), by the walled scene's route. Each sees its first waypoint, (3.5, 4.5),
    // and not the second or the goal past the wall; from the second, (6.5, 4.5), the straight
    // line to each goal is clear, so each ends on its own.
    let mut game = walled(&[]);
    let party = ordered(at(8, 0, 1));
    let members = [1, 2, 3].map(|x| game.member(at(x, 0, 1), at(x + 6, 0, 1), party));
    let before = game.searches();
    game.step();
    assert_eq!(game.searches() - before, 1);
    let [first, second, goal] = over_the_wall();
    for (member, own) in members.into_iter().zip([7, 8, 9]) {
        let route = game.get_route(member);
        assert_eq!(route.ahead(), [first, second, at(own, 0, 1)], "{own}");
        assert!(route.reached() && route.asked().is_none());
    }

    // A move into the box of (1, 1), (2, 1) and (9, 5), centre (5, 3): every goal is (8, 1),
    // and (2, 1) is nearest the centre, √13 m off, against √20 m. The unit at
    // (9, 5) sees the goal itself past the wall's corner, so its route is the goal alone.
    let mut game = walled(&[]);
    let party = ordered(goal);
    let ids = [at(1, 0, 1), at(2, 0, 1), at(9, 0, 5)].map(|at| game.member(at, goal, party));
    game.step();
    assert_eq!(game.searches(), 1);
    let routes = ids.map(|id| game.get_route(id).ahead().to_vec());
    assert_eq!(
        routes,
        [
            over_the_wall().to_vec(),
            over_the_wall().to_vec(),
            vec![goal]
        ]
    );
}

#[test]
fn a_member_whose_ask_waited_from_an_earlier_tick_plans_with_its_party() {
    // One walker asked alone in tick 0 for (9, 5), and waits; in tick 3 it joins a party with
    // another, and keeps its first tick. The two still search once.
    let mut game = walled(&[]);
    let goal = at(8, 0, 1);
    let early = game.unit(at(2, 0, 1), Some(at(9, 0, 5)));
    game.part_mut::<Route>(early)
        .ask(at(9, 0, 5), Tick::new(0), None);
    *game.part_mut::<Destination>(early) = Destination::to(Some(goal));
    let party = ordered(goal);
    game.part_mut::<Route>(early)
        .ask(goal, Tick::new(3), Some(party));
    assert_eq!(game.get_route(early).asked(), Some(Tick::new(0)));
    let late = game.unit(at(3, 0, 1), Some(goal));
    game.part_mut::<Route>(late)
        .ask(goal, Tick::new(3), Some(party));
    game.step();
    assert_eq!(game.searches(), 1);
    for id in [early, late] {
        assert_eq!(game.get_route(id).ahead(), over_the_wall());
    }
}

#[test]
fn a_member_that_sees_no_waypoint_plans_its_own_route() {
    // A second wall from (2, 4) to (3, 6) hides (1, 5) from the shared route's waypoints:
    // the line to each crosses column 2 in row 4. Of the box of (2, 1) and (1, 5), centre
    // (1.5, 3), the member at (2, 1), the lower id of the two as near, plans for both, and
    // the member at (1, 5) searches its own route to its goal, (7.5, 3). The first's goal,
    // (8.5, −1), is taken into the bounds, (8.5, 0), which it sees from (6.5, 4.5).
    let mut game = walled(&[([2, 4], [3, 6])]);
    let party = ordered(at(8, 0, 1));
    let near = game.member(at(2, 0, 1), point(17, 0), party);
    let hidden = game.member(at(1, 0, 5), point(15, 6), party);
    game.step();
    assert_eq!(game.searches(), 2);
    let [first, second, _] = over_the_wall();
    assert_eq!(game.get_route(near).ahead(), [first, second, point(17, 0)]);
    let own = game.get_route(hidden);
    assert_eq!(own.ahead().last(), Some(&point(15, 6)));
    assert!(own.reached() && !own.ahead().contains(&first));
}

#[test]
fn a_party_searches_once_for_each_layer_it_moves_on() {
    // Two walkers of no body on the ground at (2, 1) and (3, 1), two of a quarter meter in the
    // air at (2, 1) and (2, 2); no wall blocks the air, so the air's route is the goal alone. The
    // ground's is searched from (2, 1), the lower id of the two as near the centre, (2.5, 1.5).
    let mut game = walled(&[]);
    let goal = at(8, 0, 1);
    let party = ordered(goal);
    let ground = [2, 3].map(|x| game.member(at(x, 0, 1), goal, party));
    let body = Body::new(Num::QUARTER).unwrap().on(AIR);
    let air = [1, 2].map(|z| {
        let id = game.body_on(at(2, 0, z), Some(goal), Some(Num::ONE), body);
        game.part_mut::<Route>(id)
            .ask(goal, Tick::new(0), Some(party));
        id
    });
    game.step();
    assert_eq!(game.searches(), 2);
    for id in ground {
        assert_eq!(game.get_route(id).ahead(), over_the_wall());
    }
    for id in air {
        assert_eq!(game.get_route(id).ahead(), [goal]);
    }
}

#[test]
fn a_party_whose_tests_pass_the_ticks_limit_finishes_in_the_next_tick() {
    // Fifty walkers of no body at (2.5, 3.5), all to (8.5, 3.5), over 1 m cells from (0, 0) to
    // (10, 6) with no wall, whose 60 cells limit a tick's work. A line test with no wall costs 1.
    // The search costs 13: the test that the goal is clear, the 7 cells of row 3 from column 2
    // to 8 expanded, and the smoothing's 5 tests, of the lines to the 5th cell back to the 1st.
    // Its route is the goal alone, and each member's one test is the line to it. The first
    // member's runs after the search, at 14; before member m, the work is 13 + m, which meets the
    // limit at m = 47. So members 0 to 46 take their routes, and 47 to 49 wait for the next
    // tick, which searches once more, for their own party.
    let mut game = walk_match();
    game.load_open_pathing(Num::ONE, [0, 0], [10, 6], vec![ground(Num::ZERO)]);
    let goal = point(17, 7);
    let party = ordered(goal);
    let members: Vec<StableId> = (0..50)
        .map(|_| game.member(point(5, 7), goal, party))
        .collect();
    let waiting = |game: &Match| -> Vec<usize> {
        (0..members.len())
            .filter(|&at| game.get_route(members[at]).asked().is_some())
            .collect()
    };
    game.step();
    assert_eq!(waiting(&game), [47, 48, 49]);
    assert_eq!(game.searches(), 1);
    game.step();
    assert!(waiting(&game).is_empty());
    assert_eq!(game.searches(), 2);
    assert!(members.iter().all(|&id| game.get_route(id).reached()));
}

#[test]
fn a_spawn_group_on_its_path_shares_a_route_and_a_unit_off_it_plans_its_own() {
    // Three walkers at (2, 1) on their path, bound for (8, 1). As one spawn group they search
    // once; each of a group of its own, three times; one of the group that left its path, by
    // itself.
    for (groups, left, searches) in [
        ([0, 0, 0], false, 1),
        ([0, 1, 2], false, 3),
        ([0, 0, 0], true, 2),
    ] {
        let mut game = walled(&[]);
        let ids: Vec<StableId> = (0..3)
            .map(|_| game.unit(at(2, 0, 1), Some(at(8, 0, 1))))
            .collect();
        for (at, &id) in ids.iter().enumerate() {
            let mut walker = PathWalker::start(PathEnd::Start, ids[groups[at]]);
            if left && at == 2 {
                walker.leave();
            }
            game.insert_parts(id, (walker, OnPath::new(PathId::new(0))));
        }
        game.step();
        assert_eq!(game.searches(), searches, "{groups:?} {left}");
        for id in ids {
            assert_eq!(game.get_route(id).ahead(), over_the_wall());
        }
    }
}
