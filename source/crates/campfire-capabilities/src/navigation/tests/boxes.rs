use super::*;
use crate::geometry::body_box::BodyBox;

const HALF: Num = Num::HALF;
const QUARTER: Num = Num::QUARTER;

/// A box body of `width` by `height` meters, turned `angle` degrees.
fn boxed(width: i64, height: i64, angle: i64) -> Body {
    let body = BodyBox::new([Num::int(width), Num::int(height)], Num::int(angle));
    Body::boxed(body.unwrap())
}

/// The point `(x, z)` in quarters of a meter, on the ground.
fn quarters(x: i64, z: i64) -> Position {
    let quarter = |value: i64| Num::from_bits(value * ONE / 4);
    Position::new(Vec3::new(quarter(x), Num::ZERO, quarter(z))).unwrap()
}

/// Whether the circle of `radius` at `at` comes into `body`, a box at `centre`, by more than
/// two bits, as a push of collision rounds.
fn sunk(at: Position, radius: Num, centre: Position, body: &Body) -> bool {
    let Shape::Box(boxed) = body.shape() else {
        panic!("a box");
    };
    boxed.nearest(centre, at, radius - Num::from_bits(2)) == Ordering::Less
}

#[test]
fn a_walker_routes_round_a_box_and_never_stands_inside_it() {
    // Half-meter cells over ±8 m; a box of 4 by 1 m turned 30° at the origin, across the way of
    // a walker of 0.5 m from (-6, 0) to (6, 0), a quarter meter a tick. It goes round, never
    // inside the box, its center at least 0.5 m off it every tick, and arrives.
    let mut walk = Walk::new();
    walk.load_pathing(HALF, [-8, -8], [8, 8], vec![ground(HALF)]);
    let body = boxed(4, 1, 30);
    walk.body_on(at(0, 0, 0), None, None, body);
    let walker = walk.body(at(-6, 0, 0), Some(at(6, 0, 0)), Some(QUARTER), HALF);
    let mut widest = Num::ZERO;
    for _ in 0..200 {
        walk.sim.step();
        let place = *walk.sim.get::<Position>(walker);
        assert!(!sunk(place, HALF, at(0, 0, 0), &body), "{place:?}");
        widest = widest.max(place.get().z.max(-place.get().z));
    }
    assert_eq!(*walk.sim.get::<Position>(walker), at(6, 0, 0));
    // The box reaches 1.43 m off the line along z; the walker went at least that far round it.
    assert!(widest > Num::int(1), "{widest:?}");
}

#[test]
fn collision_pushes_a_walker_out_of_a_box_to_touch_it() {
    // A box of 4 by 2 m at the origin, square to the axes, and a walker of 1 m that stands half a
    // meter into its edge at x = 2: the Collide stage moves it out to touch, to x = 3, exactly, and
    // leaves the box where it stands.
    let mut walk = Walk::new();
    let building = walk.body_on(at(0, 0, 0), None, None, boxed(4, 2, 0));
    let walker = walk.body(quarters(10, 0), None, Some(QUARTER), Num::ONE);
    walk.sim.step();
    assert_eq!(*walk.sim.get::<Position>(walker), at(3, 0, 0));
    assert_eq!(*walk.sim.get::<Position>(building), at(0, 0, 0));
}

#[test]
fn a_box_that_spawns_over_a_walker_moves_it_to_the_nearest_open_cell() {
    // Half-meter cells over ±4 m and a walker of 0.25 m at (0.25, 0.25), as a box of 2 by 2 m
    // spawns at the origin. The cells whose centers come closer than 0.25 m to the box are
    // blocked to it: the nearest open centers are (1.25, 0.25) and (0.25, 1.25), each 1 m off,
    // and the first, in the lower row, is the lower cell. It goes there as the box spawns, not by
    // collision.
    let mut walk = Walk::new();
    walk.load_pathing(HALF, [-4, -4], [4, 4], vec![ground(QUARTER)]);
    let walker = walk.body(quarters(1, 1), None, Some(QUARTER), QUARTER);
    walk.sim.step();
    let building = walk.sim.spawn(at(0, 0, 0), boxed(2, 2, 0));
    let entity = walk.sim.entity(building);
    Navigation::make_room(&mut walk.sim.world, entity);
    assert_eq!(*walk.sim.get::<Position>(walker), quarters(5, 1));
    // With no pathing grid, the box pushes it out through its nearest edge, x = 1, to touch.
    let mut bare = Walk::new();
    let walker = bare.body(quarters(1, 0), None, Some(QUARTER), QUARTER);
    let building = bare.sim.spawn(at(0, 0, 0), boxed(2, 2, 0));
    let entity = bare.sim.entity(building);
    Navigation::make_room(&mut bare.sim.world, entity);
    assert_eq!(*bare.sim.get::<Position>(walker), quarters(5, 0));
}

#[test]
fn a_dash_at_a_box_ends_touching_it() {
    // A walker of 0.5 m dashes a meter a tick at a box of 2 by 2 m at the origin from (-5, 0):
    // at its nearest point, (-1, 0), each tick, through -4, -3 and -2, to -1.5, where the
    // bodies touch, in tick 3, and the dash ends.
    let mut walk = Walk::new();
    let building = walk.body_on(at(0, 0, 0), None, None, boxed(2, 2, 0));
    let dasher = walk.body(at(-5, 0, 0), None, Some(QUARTER), HALF);
    walk.sim.insert(
        dasher,
        ForcedMove::Dash {
            to: DashTo::Unit(building),
            step: Num::ONE,
            delivers: None,
        },
    );
    let mut path = Vec::new();
    for _ in 0..4 {
        walk.sim.step();
        path.push(*walk.sim.get::<Position>(dasher));
    }
    assert_eq!(
        path,
        [at(-4, 0, 0), at(-3, 0, 0), at(-2, 0, 0), quarters(-6, 0)]
    );
    assert!(walk.sim.try_get::<ForcedMove>(dasher).is_none());
}

#[test]
fn a_placed_box_needs_room_from_the_walls_and_the_bodies_that_stand() {
    // Boxes of 2 by 2 m, a tower of 0.9 m at (5, 2), and a building from (6, 0) to (8, 1), in the
    // corridor from (0, 0) to (10, 4). A box at (3, 2) ends 1.1 m from the tower's center, clear
    // of its 0.9 m; one at (4.2, 2) overlaps it. One at (7, 1.5) overlaps the building, and one at
    // (7, 2.5) clears it. One at (9.5, 2) reaches past the bounds. Two side by side at (1, 2) and
    // (3, 2) touch, and have room; at (1, 2) and (2.5, 2) they overlap, and the second is refused.
    let point = MapPoint::ground;
    let rules = NavigationRules::default();
    let body_of = |unit_type: &str| match unit_type {
        "crate" => BodyForm::boxed([Num::int(2), Num::int(2)]),
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
