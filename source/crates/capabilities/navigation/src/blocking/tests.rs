use campfire_sim::IdAllocator;

use super::*;

/// A body of radius 5/8 m at (`x`, `z`) on the first layer, walking along `way` if any a step of
/// `pace`, of weight 1, of team 0.
fn unit(id: StableId, (x, z): (Num, Num), way: Option<[i128; 2]>, pace: Num) -> Blocker {
    Blocker {
        id,
        position: Vec3::new(x, Num::ZERO, z),
        radius: Num::int(5) / 8,
        layer: Layer::FIRST,
        team: Some(Team::new(0)),
        way: way.and_then(Direction::of),
        pace,
        leaving: false,
        gathering: false,
        weight: Num::ONE,
        into: None,
    }
}

/// The step a walker at the origin, going along +x a step of 2 m, may take past `others`.
fn step_past(walker: Blocker, others: &[Blocker]) -> Option<Num> {
    let mut blocking = Blocking::default();
    blocking.read([walker].into_iter().chain(others.iter().copied()));
    let way = Going {
        way: Direction::of([1, 0]).unwrap(),
        leg: None,
    };
    blocking.brake(walker.id, &way, Num::int(2)).step
}

#[test]
fn a_walker_brakes_to_touch_a_unit_in_its_cone_on_by_its_advance() {
    // Radii of 5/8 m, two of them 1.25 m: a unit 0.75 m off the walker's way along +x, 2 m
    // ahead, is touched 2 − √(1.25² − 0.75²) = 1 m on. Standing, the walker steps 1 m of its 2;
    // one that is to step 0.5 m along +x lets it 1.5 m; one that comes 0.5 m toward it, 1 m. Ahead 3 m, a touch 2 m on is past the step; 1.25 m off the way, the bodies only touch
    // as they pass: neither brakes it.
    let mut ids = IdAllocator::default();
    let walker = unit(
        ids.allocate(),
        (Num::ZERO, Num::ZERO),
        Some([1, 0]),
        Num::ZERO,
    );
    let other = ids.allocate();
    let quarter = Num::QUARTER;
    let at = |x: Num, z: Num| (x, z);
    let standing = |x, z| unit(other, at(x, z), None, Num::ZERO);
    let walking = |way| unit(other, at(Num::int(2), quarter * 3), Some(way), Num::HALF);
    assert_eq!(
        step_past(walker, &[standing(Num::int(2), quarter * 3)]),
        Some(Num::ONE)
    );
    assert_eq!(
        step_past(walker, &[walking([1, 0])]),
        Some(Num::ONE + Num::HALF)
    );
    assert_eq!(step_past(walker, &[walking([-1, 0])]), Some(Num::ONE));
    assert_eq!(
        step_past(walker, &[standing(Num::int(3), quarter * 3)]),
        None
    );
    assert_eq!(
        step_past(walker, &[standing(Num::int(2), quarter * 5)]),
        None
    );
    // Of two in its way, the nearer brakes it: one 1.5 m ahead on the way is touched 0.25 m on.
    let near = unit(
        ids.allocate(),
        at(Num::ONE + Num::HALF, Num::ZERO),
        None,
        Num::ZERO,
    );
    assert_eq!(
        step_past(walker, &[standing(Num::int(2), quarter * 3), near]),
        Some(quarter)
    );
}

#[test]
fn a_unit_that_walks_blocks_within_45_degrees_and_one_that_stands_within_33_69() {
    // A unit at (1, 1) lies 45° off the way: one that walks blocks, touched
    // 1 − √(1.25² − 1²) = 0.25 m on, and one that stands does not, past 33.69°; a bit farther to
    // the side, neither does one that walks. A unit at (1.5, 1), of tangent 2/3, blocks standing,
    // touched 1.5 − √(1.25² − 1²) = 0.75 m on, and a bit farther to the side does not.
    let mut ids = IdAllocator::default();
    let walker = unit(
        ids.allocate(),
        (Num::ZERO, Num::ZERO),
        Some([1, 0]),
        Num::ZERO,
    );
    let other = ids.allocate();
    let bit = Num::from_bits(1);
    let at = |x: Num, z: Num, walks: bool| unit(other, (x, z), walks.then_some([0, 1]), Num::ZERO);
    let one = Num::ONE;
    let half = Num::HALF;
    assert_eq!(step_past(walker, &[at(one, one, true)]), Some(Num::QUARTER));
    assert_eq!(step_past(walker, &[at(one, one, false)]), None);
    assert_eq!(step_past(walker, &[at(one, one + bit, true)]), None);
    let narrow = at(one + half, one, false);
    assert_eq!(step_past(walker, &[narrow]), Some(Num::QUARTER * 3));
    assert_eq!(step_past(walker, &[at(one + half, one + bit, false)]), None);
    assert!(step_past(walker, &[at(one + half, one + bit, true)]).is_some());
}

#[test]
fn a_unit_walked_into_passed_through_or_of_another_layer_blocks_nothing() {
    // A unit standing 1.5 m ahead on the way, touched 0.25 m on, brakes the walker but for the
    // unit its walk goes into, one whose walk goes into the walker, one of another layer, one of
    // its team that leaves its producer, or one in its gather loop as the walker is.
    let mut ids = IdAllocator::default();
    let walker = unit(
        ids.allocate(),
        (Num::ZERO, Num::ZERO),
        Some([1, 0]),
        Num::ZERO,
    );
    let ahead = unit(
        ids.allocate(),
        (Num::ONE + Num::HALF, Num::ZERO),
        None,
        Num::ZERO,
    );
    assert_eq!(step_past(walker, &[ahead]), Some(Num::QUARTER));
    let into = Blocker {
        into: Some(ahead.id),
        ..walker
    };
    assert_eq!(step_past(into, &[ahead]), None);
    let back = Blocker {
        into: Some(walker.id),
        ..ahead
    };
    assert_eq!(step_past(walker, &[back]), None);
    let air = Blocker {
        layer: Layer::new(1),
        ..ahead
    };
    assert_eq!(step_past(walker, &[air]), None);
    let leaving = Blocker {
        leaving: true,
        ..ahead
    };
    assert_eq!(step_past(walker, &[leaving]), None);
    let gathering = |unit: Blocker| Blocker {
        gathering: true,
        ..unit
    };
    assert_eq!(step_past(gathering(walker), &[gathering(ahead)]), None);
    assert_eq!(step_past(walker, &[gathering(ahead)]), Some(Num::QUARTER));
}

#[test]
fn of_two_head_on_the_lighter_then_the_higher_id_steps_aside_by_the_two_radii() {
    // Two walkers touching head on, 1.25 m apart along x, each going at the other: each brakes to
    // a stop, and the one of the higher id steps aside, by 1.25 m; of weights 2 and 1, the lighter
    // does, whatever its id. A unit that stands where the other was brakes the walker to a stop,
    // and no one steps aside.
    let mut ids = IdAllocator::default();
    let (low, high) = (ids.allocate(), ids.allocate());
    let reach = Num::ONE + Num::QUARTER;
    let brakes = |weights: [i64; 2]| {
        let west = Blocker {
            weight: Num::int(weights[0]),
            ..unit(low, (Num::ZERO, Num::ZERO), Some([1, 0]), Num::ZERO)
        };
        let east = Blocker {
            weight: Num::int(weights[1]),
            ..unit(high, (reach, Num::ZERO), Some([-1, 0]), Num::ZERO)
        };
        let mut blocking = Blocking::default();
        blocking.read([west, east]);
        let step = Num::int(2);
        [(low, [1, 0]), (high, [-1, 0])].map(|(id, way)| {
            let way = Going {
                way: Direction::of(way).unwrap(),
                leg: None,
            };
            blocking.brake(id, &way, step)
        })
    };
    let stop = |aside| Brake {
        step: Some(Num::ZERO),
        aside,
    };
    assert_eq!(brakes([1, 1]), [stop(None), stop(Some(reach))]);
    assert_eq!(brakes([1, 2]), [stop(Some(reach)), stop(None)]);
    let mut blocking = Blocking::default();
    let walker = unit(low, (Num::ZERO, Num::ZERO), Some([1, 0]), Num::ZERO);
    blocking.read([walker, unit(high, (reach, Num::ZERO), None, Num::ZERO)]);
    let way = Going {
        way: Direction::of([1, 0]).unwrap(),
        leg: None,
    };
    let brake = blocking.brake(low, &way, Num::int(2));
    assert_eq!(brake, stop(None));
}
