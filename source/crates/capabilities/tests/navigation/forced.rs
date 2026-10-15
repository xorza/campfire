use campfire_common::Ticks;
use campfire_deliveries::Deliveries;
use campfire_script::{Effect, Frame};
use campfire_sim::{SimComponent, SnapshotError};
use campfire_units::{DashTo, ForcedMove, MoveStep};
use campfire_values::{ActionId, DashDelivery, Rank};

use super::*;

/// Half a meter.
const HALF: Num = Num::from_bits(ONE / 2);

fn at(game: &Match, id: StableId) -> Position {
    *game.part::<Position>(id)
}

fn forced(game: &Match, id: StableId) -> Option<ForcedMove> {
    game.try_part::<ForcedMove>(id).copied()
}

/// Applies `effect` as a call's would, before the next tick.
fn apply(game: &mut Match, effect: NavigationEffect) {
    let mut frame = Frame::in_tick(game.now());
    effect.apply(game, &mut frame);
}

#[test]
fn a_dash_moves_its_unit_exactly_to_a_point_or_to_the_unit_it_follows() {
    let mut game = walk_match();
    // A dash of 2 m a tick to (5, 9, 0) goes on the ground plane, at its own height: 2 m, 4 m,
    // then the last meter in tick 2, where it ends. Its unit keeps its destination, (0, 0, 9),
    // and takes no step of its own, so it stays on z = 0; once the dash ends it asks its route
    // there again in tick 2, and walks it from tick 3.
    let dasher = Navigation::unit(&mut game, (0, 0, 0), Some(Position::from((0, 0, 9))));
    let to = DashTo::Point(Position::from((5, 9, 0)));
    game.insert_parts(
        dasher,
        ForcedMove::Dash {
            to,
            step: Num::int(2),
            delivers: None,
        },
    );
    // A walker of 0.5 m walks a meter a tick along x from (10, 0, 20); a chaser of 0.5 m dashes 3
    // m a tick at it from (0, 0, 20). The walker steps first each tick: 11, 12, 13, 14, 15; the
    // chaser 3, 6, 9, 12, and in tick 4, 3 m off, within its step and the bodies' 1 m, it comes
    // to 14, where their bodies touch, and ends.
    let walker = Navigation::body(
        &mut game,
        (10, 0, 20),
        Some(Position::from((30, 0, 20))),
        Some(Num::ONE),
        HALF,
    );
    let chaser = Navigation::body(&mut game, (0, 0, 20), None, Some(Num::ONE), HALF);
    let to = DashTo::Unit(walker);
    game.insert_parts(
        chaser,
        ForcedMove::Dash {
            to,
            step: Num::int(3),
            delivers: None,
        },
    );
    let mut dash_path = Vec::new();
    let mut chase_path = Vec::new();
    for tick in 0..5 {
        game.step();
        // A forced move under way is state, restored with the rest.
        if tick == 0 {
            let mut restored = walk_match();
            Release::restore_into(&game, &mut restored);
            assert_eq!(forced(&restored, chaser), forced(&game, chaser));
        }
        dash_path.push(at(&game, dasher));
        chase_path.push((at(&game, chaser), at(&game, walker)));
    }
    let x = |x: i16| Position::from((x, 0, 0));
    assert_eq!(dash_path, [x(2), x(4), x(5), dash_path[3], dash_path[4]]);
    assert_ne!(dash_path[3], x(5));
    assert_eq!(
        game.part::<Destination>(dasher).get(),
        Some(Position::from((0, 0, 9)))
    );
    let row = |x: i16| Position::from((x, 0, 20));
    let pairs = [(3, 11), (6, 12), (9, 13), (12, 14), (14, 15)];
    assert_eq!(
        chase_path,
        pairs.map(|(chaser, walker)| (row(chaser), row(walker)))
    );
    assert_eq!([dasher, chaser].map(|id| forced(&game, id)), [None, None]);
}

#[test]
fn a_restore_refuses_a_forced_move_whose_unit_cannot_take_it_or_whose_numbers_overflow() {
    // The restore of a match whose one unit, a walker if `walks`, moves by `forced`: none when
    // it restores, else its refusal.
    let refusal = |walks: bool, forced: ForcedMove| {
        let mut game = walk_match();
        let step = walks.then_some(Num::ONE);
        let id = Navigation::body(&mut game, (0, 0, 0), None, step, HALF);
        game.insert_parts(id, forced);
        (Release::refusal_into(&game, &mut walk_match()), id)
    };
    let dash = |step: Num, delivers: Option<DashDelivery>| ForcedMove::Dash {
        to: DashTo::Point(Position::from((5, 0, 0))),
        step,
        delivers,
    };
    let knock_back = |left: u64| ForcedMove::KnockBack {
        to: Vec3::from((Num::int(3), Num::ZERO, Num::ZERO)),
        left,
    };
    let invalid = |id| {
        Some(SnapshotError::Invalid {
            ty: ForcedMove::NAME,
            id: Some(id),
        })
    };
    // A dash of twice the world's bound a tick, 2²¹ m, and a knock back of `Tick::LIMIT` ticks
    // restore; one bit or one tick more is refused, as a step past a body's reach and a share of
    // the way would overflow.
    let reach = Num::from_bits(2 * Position::BOUND.to_bits());
    let past = Num::from_bits(reach.to_bits() + 1);
    assert_eq!(refusal(true, dash(reach, None)).0, None);
    let (refused, id) = refusal(true, dash(past, None));
    assert_eq!(refused, invalid(id));
    assert_eq!(refusal(true, knock_back(Tick::LIMIT.get())).0, None);
    let (refused, id) = refusal(true, knock_back(Tick::LIMIT.get() + 1));
    assert_eq!(refused, invalid(id));
    // A unit that does not walk takes no forced move: it lacks the move step one moves in place
    // of.
    let (refused, id) = refusal(false, dash(Num::ONE, None));
    let lacks = SnapshotError::Lacks {
        holder: ForcedMove::NAME,
        needs: MoveStep::NAME,
        id,
    };
    assert_eq!(refused, Some(lacks));
    // A dash that delivers an action the book lacks, as this match's empty book lacks the first,
    // has no hooks for its end to run.
    let delivery = |source| DashDelivery::new(source, ActionId::new(0), Rank::FIRST, None);
    let mut game = walk_match();
    let id = Navigation::body(&mut game, (0, 0, 0), None, Some(Num::ONE), HALF);
    game.insert_parts(id, dash(Num::ONE, Some(delivery(id))));
    let refused = Release::refusal_into(&game, &mut walk_match());
    assert_eq!(
        refused,
        Some(SnapshotError::Invalid {
            ty: ForcedMove::NAME,
            id: Some(id),
        })
    );
}

#[test]
fn a_knock_back_moves_its_unit_away_by_equal_shares_and_a_new_move_replaces_it() {
    let mut game = walk_match();
    // 3 m over 3 ticks away from (0, 0, −2): along z, a meter a tick, the last tick on its end.
    // One off its own place goes along x.
    let off_point = Navigation::unit(&mut game, (0, 0, 0), None);
    let off_itself = Navigation::unit(&mut game, (10, 0, 0), None);
    for (id, from) in [
        (off_point, Position::from((0, 0, -2))),
        (off_itself, Position::from((10, 0, 0))),
    ] {
        apply(
            &mut game,
            NavigationEffect::KnockBack {
                id,
                from,
                distance: Num::int(3),
                ticks: Ticks::new(3),
            },
        );
    }
    game.step();
    assert_eq!(at(&game, off_point), Position::from((0, 0, 1)));
    assert_eq!(at(&game, off_itself), Position::from((11, 0, 0)));
    // A dash of 2 m a tick to (11, 0, 4) replaces the second's knock back: (11, 0, 2), then
    // (11, 0, 4), where it ends.
    apply(
        &mut game,
        NavigationEffect::Dash {
            id: off_itself,
            to: DashTo::Point(Position::from((11, 0, 4))),
            step: Num::int(2),
            delivers: None,
        },
    );
    game.step();
    assert_eq!(at(&game, off_point), Position::from((0, 0, 2)));
    assert_eq!(at(&game, off_itself), Position::from((11, 0, 2)));
    game.step();
    assert_eq!(at(&game, off_point), Position::from((0, 0, 3)));
    assert_eq!(at(&game, off_itself), Position::from((11, 0, 4)));
    assert_eq!(
        [off_point, off_itself].map(|id| forced(&game, id)),
        [None, None]
    );

    // A third of a meter a tick: 1 m over 3 ticks, each a share of what is left, 1/3 m rounded
    // once to 5 592 405 bits, then (2²⁴ − 5 592 405) / 2 = 5 592 405.5 to 5 592 406, and the
    // rest, 5 592 405 bits, in the last tick, on the end.
    let thirds = Navigation::unit(&mut game, (0, 0, 10), None);
    apply(
        &mut game,
        NavigationEffect::KnockBack {
            id: thirds,
            from: Position::from((-1, 0, 10)),
            distance: Num::ONE,
            ticks: Ticks::new(3),
        },
    );
    let mut moved = Vec::new();
    for _ in 0..3 {
        game.step();
        moved.push(at(&game, thirds).get().x.to_bits());
    }
    assert_eq!(moved, [5_592_405, 5_592_405 + 5_592_406, ONE]);
}

#[test]
fn a_forced_move_stops_before_a_static_body_and_on_the_bounds() {
    let mut game = walk_match();
    game.resource_mut::<MapBook>().bounds =
        Bounds::new([Num::int(-4), Num::int(-4)], [Num::int(4), Num::int(4)]).unwrap();
    // A tower of 1 m at (3, 0, 0). A dash of 2 m a tick from (−3, 0, 0) to (4, 0, 0), its body
    // 0.5 m: −1, then 1; the step to 3 would come within 1.5 m of the tower, so it stays on 1
    // and ends.
    Navigation::body(&mut game, (3, 0, 0), None, None, Num::ONE);
    let dasher = Navigation::body(&mut game, (-3, 0, 0), None, Some(Num::ONE), HALF);
    game.insert_parts(
        dasher,
        ForcedMove::Dash {
            to: DashTo::Point(Position::from((4, 0, 0))),
            step: Num::int(2),
            delivers: None,
        },
    );
    // A knock back of 9 m over 3 ticks from (−2, 0, 2) along x: 1, then 4, on the bound, then
    // 7, past it: it stops on 4, and ends a tick early. Its way passes 2 m from the tower.
    let knocked = Navigation::unit(&mut game, (-2, 0, 2), None);
    apply(
        &mut game,
        NavigationEffect::KnockBack {
            id: knocked,
            from: Position::from((-3, 0, 2)),
            distance: Num::int(9),
            ticks: Ticks::new(3),
        },
    );
    let mut path = Vec::new();
    for _ in 0..3 {
        game.step();
        let forced = [dasher, knocked].map(|id| forced(&game, id).is_some());
        path.push(([dasher, knocked].map(|id| at(&game, id)), forced));
    }
    assert_eq!(
        path,
        [
            (
                [Position::from((-1, 0, 0)), Position::from((1, 0, 2))],
                [true, true]
            ),
            (
                [Position::from((1, 0, 0)), Position::from((4, 0, 2))],
                [true, true]
            ),
            (
                [Position::from((1, 0, 0)), Position::from((4, 0, 2))],
                [false, false]
            ),
        ]
    );
}

#[test]
fn a_teleport_puts_its_unit_where_it_may_stand_at_once_and_ends_its_move() {
    let mut game = walk_match();
    Navigation::load_open_pathing(
        &mut game,
        Num::ONE,
        [-4, -4],
        [4, 4],
        vec![ground(Num::ZERO)],
    );
    game.resource_mut::<MapBook>().bounds =
        Bounds::new([Num::int(-4), Num::int(-4)], [Num::int(4), Num::int(4)]).unwrap();
    // A tower of 1 m at the origin blocks the four cells round it, whose centers are √0.5 m off.
    Navigation::body(&mut game, (0, 0, 0), None, None, Num::ONE);
    let id = Navigation::unit(&mut game, (-3, 0, -3), Some(Position::from((3, 0, 3))));
    game.insert_parts(
        id,
        ForcedMove::Dash {
            to: DashTo::Point(Position::from((3, 0, -3))),
            step: HALF,
            delivers: None,
        },
    );
    game.step();
    // On the tower it may not stand: the nearest cells it may, √2.5 m off, are eight, and the
    // lowest numbered, in row 2 from z = −4 and column 3 from x = −4, has its center at
    // (−0.5, 0, −1.5), at the height asked. It ends the dash, and asks its route again.
    apply(
        &mut game,
        NavigationEffect::Teleport {
            id,
            to: Position::from((0, 2, 0)),
        },
    );
    let center = Position::new(Vec3::new(-HALF, Num::int(2), Num::from_bits(-3 * ONE / 2)));
    assert_eq!(at(&game, id), center.unwrap());
    assert_eq!(forced(&game, id), None);
    assert_eq!(game.part::<Route>(id).asked(), Some(Tick::new(1)));
    // Past the bounds, on the nearest point of them, where it may stand.
    apply(
        &mut game,
        NavigationEffect::Teleport {
            id,
            to: Position::from((9, 0, 1)),
        },
    );
    assert_eq!(at(&game, id), Position::from((4, 0, 1)));
}

#[test]
fn a_knock_back_stops_before_a_wall_and_a_dash_crosses_one_to_a_cell_it_may_stand_in() {
    // Over 1 m cells from (0, 0) to (12, 10), a wall from x = 5 to 7 blocks columns 5 and 6 of
    // every row. A walker of 0.75 m may not stand in a cell whose center comes closer than that
    // to the wall's squares: columns 4 and 7 too, their centers 0.5 m off, and nothing past them.
    let three_quarters = Num::from_bits(3 * ONE / 4);
    let mut game = walk_match();
    let barrier = Wall {
        layer: Layer::FIRST,
        area: Polygon::new([[5, 0], [7, 0], [7, 10], [5, 10]].map(|point| point.map(Num::int)))
            .unwrap(),
    };
    Navigation::load_walled(
        &mut game,
        Num::ONE,
        [0, 0],
        [12, 10],
        vec![ground(three_quarters)],
        &[barrier],
    );
    game.insert_resource(Deliveries::default());
    let point = |x: i64, z: i64| Position::new(Vec3::new(HALF * x, Num::ZERO, HALF * z)).unwrap();
    let unit = |game: &mut Match, x: i64, z: i64| {
        Navigation::body(game, point(x, z), None, Some(Num::ONE), three_quarters)
    };
    // A knock back of 6 m over 3 ticks along x from (1.5, 3.5): to 3.5, whose cell is open; the
    // step to 5.5 touches column 4, which the walls block, so it stays on 3.5 and ends.
    let knocked = unit(&mut game, 3, 7);
    apply(
        &mut game,
        NavigationEffect::KnockBack {
            id: knocked,
            from: point(1, 7),
            distance: Num::int(6),
            ticks: Ticks::new(3),
        },
    );
    // One that stands in column 4 at (4.5, 8.5), as collision might push it, is knocked 2 m away
    // from the wall in one tick, to 2.5: its step touches its own cell, which does not count, then
    // columns 3 and 2, open.
    let margin = unit(&mut game, 9, 17);
    apply(
        &mut game,
        NavigationEffect::KnockBack {
            id: margin,
            from: point(11, 17),
            distance: Num::int(2),
            ticks: Ticks::new(1),
        },
    );
    // A dash of 2 m a tick from (2.5, 5.5) to (10.5, 5.5) crosses the wall: 4.5, 6.5, 8.5, then
    // 10.5, where it ends in an open cell.
    let crosser = unit(&mut game, 5, 11);
    game.insert_parts(
        crosser,
        ForcedMove::Dash {
            to: DashTo::Point(point(21, 11)),
            step: Num::int(2),
            delivers: None,
        },
    );
    // A dash of 2 m a tick from (2.5, 1.5) to (6, 1.5) ends there in tick 1, in column 6. The
    // nearest cells it may stand in are columns 3 and 8 of its row, their centers 2.5 m off; the
    // lower numbered, (3.5, 1.5), takes it. The action it delivers ends there: the way its steps
    // went, 2 m and 1.5 m, along x.
    let short = unit(&mut game, 5, 3);
    let delivery = DashDelivery::new(short, ActionId::new(0), Rank::FIRST, None);
    game.insert_parts(
        short,
        ForcedMove::Dash {
            to: DashTo::Point(point(12, 3)),
            step: Num::int(2),
            delivers: Some(delivery),
        },
    );
    let mut seen = Vec::new();
    for _ in 0..4 {
        game.step();
        let units = [knocked, margin, crosser, short];
        seen.push((
            units.map(|id| at(&game, id)),
            units.map(|id| forced(&game, id).is_some()),
        ));
        let ended = &mut game.resource_mut::<Deliveries>().delivered;
        if let Some(end) = ended.pop() {
            assert_eq!(end.hit.pos, point(7, 3));
            assert_eq!(end.hit.distance, Num::from_bits(7 * ONE / 2));
            assert_eq!(
                end.hit.direction,
                Some(Vec3::new(Num::ONE, Num::ZERO, Num::ZERO))
            );
            assert_eq!(ended.len(), 0);
        }
    }
    let row = |x: [i64; 4]| {
        [
            point(x[0], 7),
            point(x[1], 17),
            point(x[2], 11),
            point(x[3], 3),
        ]
    };
    assert_eq!(
        seen,
        [
            (row([7, 5, 9, 9]), [true, false, true, true]),
            (row([7, 5, 13, 7]), [false, false, true, false]),
            (row([7, 5, 17, 7]), [false, false, true, false]),
            (row([7, 5, 21, 7]), [false, false, false, false]),
        ]
    );
}
