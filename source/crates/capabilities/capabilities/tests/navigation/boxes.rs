use campfire_units::{DashTo, ForcedMove};

use super::*;

const HALF: Num = Num::HALF;
const QUARTER: Num = Num::QUARTER;

/// A box form of `width` by `height` meters.
fn boxed(width: i64, height: i64) -> BodyForm {
    BodyForm::new(
        BodyShape::box_sized([Num::int(width), Num::int(height)]).unwrap(),
        Layer::FIRST,
    )
}

/// The point `(x, z)` in quarters of a meter, on the ground.
fn quarters(x: i64, z: i64) -> Vec3 {
    let quarter = |value: i64| Num::from_bits(value * ONE / 4);
    Vec3::new(quarter(x), Num::ZERO, quarter(z))
}

/// Whether the circle of `radius` at `position` comes into `body`, a box at `center`, by more than
/// two bits, as a push of collision rounds.
fn is_sunk(position: Vec3, radius: Num, center: Vec3, body: &Body) -> bool {
    let Shape::Box(boxed) = body.shape() else {
        panic!("a box");
    };
    boxed.closer(center, position, radius - Num::from_bits(2))
}

#[test]
fn a_walker_routes_round_a_box_and_never_stands_inside_it() {
    // Half-meter cells over ±8 m; a box of 4 by 1 m turned 30° at the origin, across the way of
    // a walker of 0.5 m from (-6, 0) to (6, 0), a quarter meter a tick. It goes round, never
    // inside the box, its center at least 0.5 m off it every tick, and arrives.
    let mut game = walk_match();
    Navigation::load_open_pathing(&mut game, HALF, [-8, -8], [8, 8], [ground(HALF)]);
    let turn = Num::int(30);
    let body = boxed(4, 1).at(turn);
    Navigation::walker_at(&mut game, (0, 0, 0))
        .angle(turn)
        .body(boxed(4, 1))
        .place(&mut game);
    let walker = Navigation::walker_at(&mut game, (-6, 0, 0))
        .circle(HALF)
        .walks(QUARTER, Some(Vec3::from((6, 0, 0))))
        .place(&mut game);
    let mut widest = Num::ZERO;
    for _ in 0..200 {
        game.step();
        let position = game.part::<Position>(walker).get();
        assert!(
            !is_sunk(position, HALF, Vec3::from((0, 0, 0)), &body),
            "{position:?}"
        );
        widest = widest.max(position.z.max(-position.z));
    }
    assert_eq!(game.part::<Position>(walker).get(), Vec3::from((6, 0, 0)));
    // The box's half sides of 2 and 0.5 m, turned 30°, reach 2 · sin 30° + 0.5 · cos 30° =
    // 1.433 m off the line along z, so the walker's center keeps 1.933 m off it as it passes;
    // the route goes through the centers of the half-meter cells, the first clear one 2.25 m off
    // the line, which the walker turns back from before it reaches it.
    let clear = Num::int(1) + Num::int(1) / Num::int(2) * Num::from_bits(14_529_495) + HALF;
    assert!(
        widest >= clear && widest <= Num::int(9) / Num::int(4),
        "{widest:?}"
    );
}

#[test]
fn a_walker_routes_round_a_box_wider_than_a_map_and_collides_with_it() {
    // A box of 1,117 by 100 m at the origin, a Zero Hour bridge's size, across the way of a
    // walker of 0.5 m from (540, −56) to (540, 56), a meter a tick, over meter cells from −600 to
    // 600 along x and −80 to 80 along z. It goes round the box's end at x = 558.5, never inside
    // it, and arrives.
    let mut game = walk_match();
    Navigation::load_open_pathing(
        &mut game,
        Num::ONE,
        [-600, -80],
        [600, 80],
        vec![ground(HALF)],
    );
    let bridge = boxed(1117, 100);
    Navigation::walker_at(&mut game, (0, 0, 0))
        .body(bridge)
        .place(&mut game);
    let bridge = bridge.at(Num::ZERO);
    let walker = Navigation::walker_at(&mut game, (540, 0, -56))
        .circle(HALF)
        .walks(Num::ONE, Some(Vec3::from((540, 0, 56))))
        .place(&mut game);
    let mut farthest = Num::ZERO;
    for _ in 0..200 {
        game.step();
        let position = game.part::<Position>(walker).get();
        assert!(
            !is_sunk(position, HALF, Vec3::from((0, 0, 0)), &bridge),
            "{position:?}"
        );
        farthest = farthest.max(position.x);
    }
    assert_eq!(
        game.part::<Position>(walker).get(),
        Vec3::from((540, 0, 56))
    );
    // The box's end lies at x = 558.5 and the walker's radius is 0.5 m, so its center keeps to
    // x ≥ 559 round it, and the route runs through the centers of meter cells: the first clear one
    // spans x from 559 to 560, its center at 559.5.
    assert_eq!(farthest, Num::int(1119) / Num::int(2));
    // A walker of 1 m that stands half a meter into its long edge at z = 50, far from its ends:
    // the Collide stage moves it out to touch, to z = 51, and a bit past, the push's slack.
    let pushed = Navigation::walker_at(&mut game, quarters(-1600, 198))
        .circle(Num::ONE)
        .walks(QUARTER, None)
        .place(&mut game);
    game.step();
    assert_eq!(
        game.part::<Position>(pushed).get(),
        past((-400, 0, 51), [0, 1])
    );
}

#[test]
fn collision_pushes_a_walker_out_of_a_box_to_touch_it() {
    // A box of 4 by 2 m at the origin, square to the axes, and a walker of 1 m that stands half a
    // meter into its edge at x = 2: the Collide stage moves it out to touch, to x = 3, and a bit
    // past, the push's slack, and leaves the box where it stands.
    let mut game = walk_match();
    let building = Navigation::walker_at(&mut game, (0, 0, 0))
        .body(boxed(4, 2))
        .place(&mut game);
    let walker = Navigation::walker_at(&mut game, quarters(10, 0))
        .circle(Num::ONE)
        .walks(QUARTER, None)
        .place(&mut game);
    game.step();
    assert_eq!(game.part::<Position>(walker).get(), past((3, 0, 0), [1, 0]));
    assert_eq!(game.part::<Position>(building).get(), Vec3::from((0, 0, 0)));
}

#[test]
fn a_walker_wedged_between_boxes_stands_clear_and_one_in_a_narrow_gap_walks_out() {
    // Half-meter cells over ±12 m, for walkers of 1 m. Boxes of 4 by 2 m at the origin and at
    // (5, 0) leave a gap of 1 m, x 2 to 3, and a walker of 1 m at (2.5, 0) overlaps both: the
    // first box's contact pushes it out to x = 3, the second's back to x = 2, less the push's
    // slack, so it stands at the cell nearest there whose center keeps 1 m off both boxes. The
    // rows within 1 m of z ±1 are blocked, so the nearest open centers lie at z ±2.25, x 1.75 a
    // quarter meter less the slack off, x 2.25 a quarter meter and the slack: in the lower row,
    // (1.75, −2.25).
    let mut game = walk_match();
    Navigation::load_open_pathing(&mut game, HALF, [-12, -12], [12, 12], [ground(Num::ONE)]);
    for x in [0, 5] {
        Navigation::walker_at(&mut game, (x, 0, 0))
            .body(boxed(4, 2))
            .place(&mut game);
    }
    game.step();
    let wedged = Navigation::walker_at(&mut game, quarters(10, 0))
        .circle(Num::ONE)
        .walks(QUARTER, None)
        .place(&mut game);
    game.step();
    assert_eq!(game.part::<Position>(wedged).get(), quarters(7, -9));

    // Boxes of 4 by 10 m at (0, 0) and (6, 0) leave a corridor 2 m wide, x 2 to 4, from z −5 to
    // 5, where every cell's center comes closer than 1 m to a box. A walker of 1 m at (3, 0)
    // touches both and overlaps neither: its route to (3, 10) leaves from the open cell nearest
    // it, out of the corridor's end, and it arrives.
    let mut game = walk_match();
    Navigation::load_open_pathing(&mut game, HALF, [-12, -12], [12, 12], [ground(Num::ONE)]);
    for x in [0, 6] {
        Navigation::walker_at(&mut game, (x, 0, 0))
            .body(boxed(4, 10))
            .place(&mut game);
    }
    game.step();
    let goal = Vec3::from((3, 0, 10));
    let walker = Navigation::walker_at(&mut game, (3, 0, 0))
        .circle(Num::ONE)
        .walks(HALF, Some(goal))
        .place(&mut game);
    let arrived = (0..80).find(|_| {
        game.step();
        game.part::<Position>(walker).get() == goal
    });
    assert!(
        arrived.is_some(),
        "{:?}",
        game.part::<Position>(walker).get()
    );
}

#[test]
fn a_box_that_spawns_over_a_walker_moves_it_to_the_nearest_open_cell() {
    // Half-meter cells over ±4 m and a walker of 0.25 m at (0.25, 0.25), as a box of 2 by 2 m
    // spawns at the origin. The cells whose centers come closer than 0.25 m to the box are
    // blocked to it: the nearest open centers are (1.25, 0.25) and (0.25, 1.25), each 1 m off,
    // and the first, in the lower row, is the lower cell. It goes there as the box spawns, not by
    // collision.
    let mut game = walk_match();
    Navigation::load_open_pathing(&mut game, HALF, [-4, -4], [4, 4], [ground(QUARTER)]);
    let walker = Navigation::walker_at(&mut game, quarters(1, 1))
        .circle(QUARTER)
        .walks(QUARTER, None)
        .place(&mut game);
    game.step();
    Navigation::walker_at(&mut game, (0, 0, 0))
        .body(boxed(2, 2))
        .place(&mut game);
    assert_eq!(game.part::<Position>(walker).get(), quarters(5, 1));
    // With no pathing grid, the box pushes it out through its nearest edge, x = 1, to touch, and
    // a bit past.
    let mut bare = walk_match();
    let walker = Navigation::walker_at(&mut bare, quarters(1, 0))
        .circle(QUARTER)
        .walks(QUARTER, None)
        .place(&mut bare);
    Navigation::walker_at(&mut bare, (0, 0, 0))
        .body(boxed(2, 2))
        .place(&mut bare);
    let touching = quarters(5, 0);
    let past_it = Vec3::new(touching.x + Num::EPSILON, touching.y, touching.z);
    assert_eq!(bare.part::<Position>(walker).get(), past_it);
}

#[test]
fn a_dash_at_a_box_ends_touching_it() {
    // A walker of 0.5 m dashes a meter a tick at a box of 2 by 2 m at the origin from (-5, 0):
    // at its nearest point, (-1, 0), each tick, through -4, -3 and -2, to -1.5, where the
    // bodies touch, in tick 3, and the dash ends.
    let mut game = walk_match();
    let building = Navigation::walker_at(&mut game, (0, 0, 0))
        .body(boxed(2, 2))
        .place(&mut game);
    let dasher = Navigation::walker_at(&mut game, (-5, 0, 0))
        .circle(HALF)
        .walks(QUARTER, None)
        .place(&mut game);
    game.insert_parts(
        dasher,
        ForcedMove::Dash {
            to: DashTo::Unit(building),
            step: Num::ONE,
        },
    );
    let mut path = Vec::new();
    for _ in 0..4 {
        game.step();
        path.push(game.part::<Position>(dasher).get());
    }
    assert_eq!(
        path,
        [
            Vec3::from((-4, 0, 0)),
            Vec3::from((-3, 0, 0)),
            Vec3::from((-2, 0, 0)),
            quarters(-6, 0)
        ]
    );
    assert!(game.try_part::<ForcedMove>(dasher).is_none());
}

/// `at`, in meters, moved by `bits` along x and z.
fn past(at: (i16, i16, i16), bits: [i64; 2]) -> Vec3 {
    Vec3::from(at) + Vec3::new(Num::from_bits(bits[0]), Num::ZERO, Num::from_bits(bits[1]))
}
