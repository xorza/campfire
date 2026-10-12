use super::*;
use crate::geometry::body_box::BodyBox;
use crate::units::forced_move::{DashTo, ForcedMove};

const HALF: Num = Num::HALF;
const QUARTER: Num = Num::QUARTER;

/// A box body of `width` by `height` meters, turned `angle` degrees.
fn boxed(width: i64, height: i64, angle: i64) -> Body {
    let body = BodyBox::new([Num::int(width), Num::int(height)], Num::int(angle));
    Body::of_box(body.unwrap())
}

/// The point `(x, z)` in quarters of a meter, on the ground.
fn quarters(x: i64, z: i64) -> Position {
    let quarter = |value: i64| Num::from_bits(value * ONE / 4);
    Position::new(Vec3::new(quarter(x), Num::ZERO, quarter(z))).unwrap()
}

/// Whether the circle of `radius` at `pos` comes into `body`, a box at `centre`, by more than
/// two bits, as a push of collision rounds.
fn sunk(pos: Position, radius: Num, centre: Position, body: &Body) -> bool {
    let Shape::Box(boxed) = body.shape() else {
        panic!("a box");
    };
    boxed.nearest(centre, pos, radius - Num::from_bits(2)) == Ordering::Less
}

#[test]
fn a_walker_routes_round_a_box_and_never_stands_inside_it() {
    // Half-meter cells over ±8 m; a box of 4 by 1 m turned 30° at the origin, across the way of
    // a walker of 0.5 m from (-6, 0) to (6, 0), a quarter meter a tick. It goes round, never
    // inside the box, its center at least 0.5 m off it every tick, and arrives.
    let mut game = walk_match();
    Navigation::load_open_pathing(&mut game, HALF, [-8, -8], [8, 8], [ground(HALF)]);
    let body = boxed(4, 1, 30);
    Navigation::body_on(&mut game, (0, 0, 0), None, None, body);
    let walker = Navigation::body(
        &mut game,
        (-6, 0, 0),
        Some(Position::from((6, 0, 0))),
        Some(QUARTER),
        HALF,
    );
    let mut widest = Num::ZERO;
    for _ in 0..200 {
        game.step();
        let pos = *game.part::<Position>(walker);
        assert!(
            !sunk(pos, HALF, Position::from((0, 0, 0)), &body),
            "{pos:?}"
        );
        widest = widest.max(pos.get().z.max(-pos.get().z));
    }
    assert_eq!(*game.part::<Position>(walker), Position::from((6, 0, 0)));
    // The box reaches 1.43 m off the line along z; the walker went at least that far round it.
    assert!(widest > Num::int(1), "{widest:?}");
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
    let bridge = boxed(1117, 100, 0);
    Navigation::body_on(&mut game, (0, 0, 0), None, None, bridge);
    let walker = Navigation::body(
        &mut game,
        (540, 0, -56),
        Some(Position::from((540, 0, 56))),
        Some(Num::ONE),
        HALF,
    );
    let mut farthest = Num::ZERO;
    for _ in 0..200 {
        game.step();
        let pos = *game.part::<Position>(walker);
        assert!(
            !sunk(pos, HALF, Position::from((0, 0, 0)), &bridge),
            "{pos:?}"
        );
        farthest = farthest.max(pos.get().x);
    }
    assert_eq!(*game.part::<Position>(walker), Position::from((540, 0, 56)));
    assert!(farthest > Num::int(559), "{farthest:?}");
    // A walker of 1 m that stands half a meter into its long edge at z = 50, far from its ends:
    // the Collide stage moves it out to touch, to z = 51.
    let pushed = Navigation::body(
        &mut game,
        quarters(-1600, 198),
        None,
        Some(QUARTER),
        Num::ONE,
    );
    game.step();
    assert_eq!(
        *game.part::<Position>(pushed),
        Position::from((-400, 0, 51))
    );
}

#[test]
fn collision_pushes_a_walker_out_of_a_box_to_touch_it() {
    // A box of 4 by 2 m at the origin, square to the axes, and a walker of 1 m that stands half a
    // meter into its edge at x = 2: the Collide stage moves it out to touch, to x = 3, exactly, and
    // leaves the box where it stands.
    let mut game = walk_match();
    let building = Navigation::body_on(&mut game, (0, 0, 0), None, None, boxed(4, 2, 0));
    let walker = Navigation::body(&mut game, quarters(10, 0), None, Some(QUARTER), Num::ONE);
    game.step();
    assert_eq!(*game.part::<Position>(walker), Position::from((3, 0, 0)));
    assert_eq!(*game.part::<Position>(building), Position::from((0, 0, 0)));
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
    let walker = Navigation::body(&mut game, quarters(1, 1), None, Some(QUARTER), QUARTER);
    game.step();
    let building = Units::spawn(&mut game, (0, 0, 0), Team::new(0), boxed(2, 2, 0));
    let entity = game.unit_entity(building);
    Navigation::make_room(&mut game, entity);
    assert_eq!(*game.part::<Position>(walker), quarters(5, 1));
    // With no pathing grid, the box pushes it out through its nearest edge, x = 1, to touch.
    let mut bare = walk_match();
    let walker = Navigation::body(&mut bare, quarters(1, 0), None, Some(QUARTER), QUARTER);
    let building = Units::spawn(&mut bare, (0, 0, 0), Team::new(0), boxed(2, 2, 0));
    let entity = bare.unit_entity(building);
    Navigation::make_room(&mut bare, entity);
    assert_eq!(*bare.part::<Position>(walker), quarters(5, 0));
}

#[test]
fn a_dash_at_a_box_ends_touching_it() {
    // A walker of 0.5 m dashes a meter a tick at a box of 2 by 2 m at the origin from (-5, 0):
    // at its nearest point, (-1, 0), each tick, through -4, -3 and -2, to -1.5, where the
    // bodies touch, in tick 3, and the dash ends.
    let mut game = walk_match();
    let building = Navigation::body_on(&mut game, (0, 0, 0), None, None, boxed(2, 2, 0));
    let dasher = Navigation::body(&mut game, (-5, 0, 0), None, Some(QUARTER), HALF);
    game.insert_parts(
        dasher,
        ForcedMove::Dash {
            to: DashTo::Unit(building),
            step: Num::ONE,
            delivers: None,
        },
    );
    let mut path = Vec::new();
    for _ in 0..4 {
        game.step();
        path.push(*game.part::<Position>(dasher));
    }
    assert_eq!(
        path,
        [
            Position::from((-4, 0, 0)),
            Position::from((-3, 0, 0)),
            Position::from((-2, 0, 0)),
            quarters(-6, 0)
        ]
    );
    assert!(game.try_part::<ForcedMove>(dasher).is_none());
}

#[test]
fn a_placed_box_needs_room_from_the_walls_and_the_bodies_that_stand() {
    // Boxes of 2 by 2 m, a tower of 0.9 m at (5, 2), and a building from (6, 0) to (8, 1), in the
    // corridor from (0, 0) to (10, 4). A box at (3, 2) ends 1.1 m from the tower's center, clear
    // of its 0.9 m; one at (4.2, 2) overlaps it. One at (7, 1.5) overlaps the building, and one at
    // (7, 2.5) clears it. One at (9.5, 2) reaches past the bounds. Two side by side at (1, 2) and
    // (3, 2) touch, and have room; at (1, 2) and (2.5, 2) they overlap, and the second is refused.
    let point = MapPoint::ground;
    let rules = ModeNavigationData::default();
    let body_of = |unit_type: &str| match unit_type {
        "crate" => BodyForm::box_sized([Num::int(2), Num::int(2)]),
        other => corridor_body(other),
    };
    let check = |boxes: &[(Num, Num)], walled: bool| {
        let mut map = corridor(&[(5, 2)], (8, 3));
        map.paths.clear();
        map.markers.clear();
        if walled {
            let corner = |x, z| point(x, z);
            map.navigation = Some(MapNavigationData {
                cell: Scalar::Decimal(Num::HALF),
                walls: vec![WallData {
                    layer: None,
                    points: vec![corner(6, 0), corner(8, 0), corner(8, 1), corner(6, 1)],
                }],
            });
        }
        let placed = boxes.iter().map(|&(x, z)| PlacedUnitData {
            pos: MapPoint::Ground([Scalar::Decimal(x), Scalar::Decimal(z)]),
            ..PlacedUnitData::new("crate", "west", point(0, 0))
        });
        map.units.extend(placed);
        check_walkable(&map, &[ground(Num::HALF)], &rules, body_of)
    };
    let blocked = |unit| {
        Err(MapProblem::BoxBlocked {
            unit,
            unit_type: DeclaredName::new("crate").unwrap(),
        })
    };
    let num = |text: &str| text.parse::<Num>().unwrap();
    // The tower and the creep come first among the placed units, at 0 and 1.
    assert_eq!(check(&[(num("3"), num("2"))], false), Ok(()));
    assert_eq!(check(&[(num("4.2"), num("2"))], false), blocked(2));
    assert_eq!(check(&[(num("7"), num("1.5"))], true), blocked(2));
    assert_eq!(check(&[(num("7"), num("2.5"))], true), Ok(()));
    assert_eq!(check(&[(num("9.5"), num("2"))], false), blocked(2));
    assert_eq!(
        check(&[(num("1"), num("2")), (num("3"), num("2"))], false),
        Ok(())
    );
    assert_eq!(
        check(&[(num("1"), num("2")), (num("2.5"), num("2"))], false),
        blocked(2)
    );
}
