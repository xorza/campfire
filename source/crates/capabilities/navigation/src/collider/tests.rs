use std::array;

use bevy_ecs::world::World;
use campfire_geometry::Radius;
use campfire_sim::IdAllocator;

use super::*;
use crate::broadphase::Broadphase;
use crate::broadphase::internals::statics;
/// Colliders of radius 1 at each place on the ground, in id order, each movable as given, and
/// walking when movable.
fn row(at: &[(Num, Num, bool)]) -> Vec<Collider> {
    let mut ids = IdAllocator::default();
    let mut world = World::new();
    at.iter()
        .map(|&(x, z, movable)| Collider {
            id: ids.allocate(),
            entity: world.spawn_empty().id(),
            point: Vec3::new(x, Num::int(2), z),
            shape: Shape::Circle(Radius::new(Num::ONE).unwrap()),
            layer: Layer::FIRST,
            movable,
            walking: movable,
            gathering: false,
            weight: Num::ONE,
            way: None,
            team: None,
            firm: false,
            leaving: false,
            into: None,
            pinned: false,
        })
        .collect()
}

/// Runs `stage` as the Collide stage does: on the walkers among `colliders`, with their contacts
/// and the static index of the others; then writes the walkers back.
fn staged<T>(
    colliders: &mut [Collider],
    stage: impl FnOnce(&mut [Collider], &[Contact], &BodyIndex) -> T,
) -> T {
    let index = statics(colliders);
    let mut walkers: Vec<Collider> = colliders.iter().filter(|c| c.movable).copied().collect();
    let contacts = Broadphase::default().contacts(&walkers, &index).to_vec();
    let result = stage(&mut walkers, &contacts, &index);
    let mut moved = walkers.into_iter();
    for collider in colliders.iter_mut().filter(|c| c.movable) {
        *collider = moved.next().unwrap();
    }
    result
}

/// Parts the bodies of `colliders` that overlap; the pairs it left deep.
fn resolve(colliders: &mut [Collider]) -> usize {
    staged(colliders, |walkers, contacts, _| {
        Collider::resolve(walkers, contacts)
    })
}

/// Pushes apart the bodies of `colliders` within their personal space, by `moves`.
fn push(colliders: &mut [Collider], moves: Moves) {
    staged(colliders, |walkers, contacts, statics| {
        Collider::push(walkers, contacts, Pushes { moves, statics });
    });
}

fn places(colliders: &[Collider]) -> Vec<(Num, Num)> {
    colliders.iter().map(|c| (c.point.x, c.point.z)).collect()
}

#[test]
fn overlapping_bodies_part_to_the_sum_of_their_radii() {
    // 1.5 m apart, radii 1 and 1: an overlap of 0.5, a quarter meter each way.
    let half = Num::HALF;
    let quarter = Num::QUARTER;
    let mut two = row(&[
        (Num::int(0), Num::int(0), true),
        (Num::int(1) + half, Num::int(0), true),
    ]);
    resolve(&mut two);
    assert_eq!(
        places(&two),
        [
            (-quarter, Num::int(0)),
            (Num::int(1) + half + quarter, Num::int(0))
        ]
    );
    assert_eq!(two[0].point.y, Num::int(2));

    // A unit that walks into one that stands takes the whole half meter, whichever id it has:
    // no unit shoves another aside.
    for walker in [0, 1] {
        let mut pair = row(&[
            (Num::int(0), Num::int(0), true),
            (Num::int(1) + half, Num::int(0), true),
        ]);
        pair[1 - walker].walking = false;
        resolve(&mut pair);
        let expected = [
            [(-half, Num::int(0)), (Num::int(1) + half, Num::int(0))],
            [(Num::int(0), Num::int(0)), (Num::int(2), Num::int(0))],
        ];
        assert_eq!(places(&pair), expected[walker], "walker {walker}");
    }

    // A tower that may not be pushed leaves the whole half meter to the walker.
    let mut tower = row(&[
        (Num::int(0), Num::int(0), false),
        (Num::int(0), Num::int(1) + half, true),
    ]);
    resolve(&mut tower);
    assert_eq!(
        places(&tower),
        [(Num::int(0), Num::int(0)), (Num::int(0), Num::int(2))]
    );

    // Touching, 2 m apart, is no overlap; two towers never move.
    for (x, movable) in [(Num::int(2), true), (Num::int(1), false)] {
        let mut pair = row(&[
            (Num::int(0), Num::int(0), movable),
            (x, Num::int(0), movable),
        ]);
        let before = places(&pair);
        resolve(&mut pair);
        assert_eq!(places(&pair), before);
    }

    // On one spot they part along x by a meter each, the higher id towards +x.
    let mut stacked = row(&[
        (Num::int(3), Num::int(3), true),
        (Num::int(3), Num::int(3), true),
    ]);
    resolve(&mut stacked);
    assert_eq!(
        places(&stacked),
        [(Num::int(2), Num::int(3)), (Num::int(4), Num::int(3))]
    );

    // On a 3-4-5 line: (0.3, 0.4), each a whole number of bits below, lie (5 033 164,
    // 6 710 886) bits apart, √70 368 730 755 892 = 8 388 607.92 bits. That offset made 2 m
    // long, 33 554 432 bits, is (20 132 657.92, 26 843 546.56), rounded up (20 132 658,
    // 26 843 547): a change of (15 099 494, 20 132 661), split (7 549 747, 10 066 330) back and
    // (7 549 747, 10 066 331) forward.
    let tenth = |n: i64| Num::from_bits((n << Num::FRAC_BITS) / 10);
    let mut slant = row(&[(Num::int(0), Num::int(0), true), (tenth(3), tenth(4), true)]);
    resolve(&mut slant);
    let bits = Num::from_bits;
    let expected = [
        (-bits(7_549_747), -bits(10_066_330)),
        (tenth(3) + bits(7_549_747), tenth(4) + bits(10_066_331)),
    ];
    assert_eq!(places(&slant), expected);

    // Centers 1 bit apart along each axis are √2 bits apart. That offset made 33 554 432 bits
    // long is 2²⁴·√2 = 23 726 566.41 bits along each axis, rounded up 23 726 567: a change of
    // 23 726 566, split 11 863 283 each way. The pair ends 23 726 567·√2 = 33 554 432.84 bits
    // apart.
    let mut close = row(&[(Num::ZERO, Num::ZERO, true), (bits(1), bits(1), true)]);
    resolve(&mut close);
    assert_eq!(
        places(&close),
        [
            (-bits(11_863_283), -bits(11_863_283)),
            (bits(11_863_284), bits(11_863_284))
        ]
    );
}

/// How far `parted`, an axis of an offset `reach` long along one whose axis is `along` and whose
/// square is `square`, lies past the exact axis, away from zero: the exact one, below 2³⁷,
/// within 2⁻¹⁰ of a bit by `f64`.
#[expect(
    clippy::float_arithmetic,
    clippy::cast_precision_loss,
    reason = "f64 is only the accuracy reference"
)]
fn past_exact(parted: i128, along: i128, reach: i128, square: i128) -> f64 {
    let exact = along.abs() as f64 * reach as f64 / (square as f64).sqrt();
    parted.abs() as f64 - exact
}

#[test]
fn parted_bodies_lie_the_sum_of_their_radii_apart_rounded_up_to_a_bit() {
    // Offsets in bits from the first body to the second, with their radii in meters. The
    // squared lengths 2, 74, 2⁴⁹ + 2²⁵ + 1 and 2⁷⁰ + 24 690·2³⁵ + 12 345² + 2⁶⁸ are not
    // squares; 81, 25·2⁴⁰ and 1 are. The last four pairs stand at `MAX_BOUND`, 2,048 m, the
    // widest reach.
    let cases: [([i64; 2], [i64; 2]); 8] = [
        ([1, 1], [1, 1]),
        ([0, -9], [1, 1]),
        ([3 << 20, -4 << 20], [1, 1]),
        ([-(1 << 24), (1 << 24) + 1], [1, 1]),
        ([-7, 5], [1, 2048]),
        ([(1 << 35) + 12_345, -(1 << 34)], [2048, 2048]),
        ([1, 0], [2048, 2048]),
        ([-1, 1], [2048, 2048]),
    ];
    for (offset, radii) in cases {
        let mut pair = row(&[
            (Num::int(5), Num::int(-5), true),
            (
                Num::int(5) + Num::from_bits(offset[0]),
                Num::int(-5) + Num::from_bits(offset[1]),
                true,
            ),
        ]);
        for (collider, radius) in pair.iter_mut().zip(radii) {
            collider.shape = Shape::Circle(Radius::new(Num::int(radius)).unwrap());
        }
        let before: [[i64; 2]; 2] =
            array::from_fn(|i| [pair[i].point.x, pair[i].point.z].map(Num::to_bits));
        resolve(&mut pair);
        let moved: [[i128; 2]; 2] = array::from_fn(|i| {
            let after = [pair[i].point.x, pair[i].point.z].map(Num::to_bits);
            [0, 1].map(|axis| i128::from(after[axis] - before[i][axis]))
        });
        let reach = i128::from((radii[0] + radii[1]) << Num::FRAC_BITS);
        let along = offset.map(i128::from);
        let square = along[0] * along[0] + along[1] * along[1];
        let parted = [0, 1].map(|axis| along[axis] + moved[1][axis] - moved[0][axis]);
        for axis in 0..2 {
            // Each axis lies from the exact |along| · reach ÷ √square to less than a bit past it,
            // away from the other body.
            let case = format!("{offset:?} with radii {radii:?}, axis {axis}");
            assert_eq!(parted[axis].signum(), along[axis].signum(), "{case}");
            let past = past_exact(parted[axis], along[axis], reach, square);
            assert!((-1e-3..1.0 + 1e-3).contains(&past), "{case} {past}");
            // Both walk, so they share the change, the second taking the odd bit.
            let change = parted[axis] - along[axis];
            assert_eq!(moved[0][axis], -(change / 2), "{case}");
            assert_eq!(moved[1][axis], change - change / 2, "{case}");
        }
        // Each coordinate under a bit outward puts the length at most √2 bits past the reach.
        let length = parted[0] * parted[0] + parted[1] * parted[1];
        let case = format!("{offset:?} with radii {radii:?}");
        assert!(
            reach.pow(2) <= length && length < (reach + 2).pow(2),
            "{case}"
        );
        assert!(!pair[0].overlaps(&pair[1]), "{case}");
    }
}

#[test]
fn two_gatherers_or_a_walker_and_the_unit_it_walks_into_pass_through_each_other() {
    // Three walkers 1.5 m apart along x, of radius 1: overlaps of 0.5. The first two gather, and
    // stay where they stand; the third, which does not, parts from the second by a quarter
    // meter each way.
    let half = Num::HALF;
    let mut colliders = row(&[
        (Num::ZERO, Num::ZERO, true),
        (Num::ONE + half, Num::ZERO, true),
        (Num::int(3), Num::ZERO, true),
    ]);
    colliders[0].gathering = true;
    colliders[1].gathering = true;
    assert!(!colliders[0].overlaps(&colliders[1]));
    assert!(colliders[1].overlaps(&colliders[2]));
    resolve(&mut colliders);
    let quarter = Num::QUARTER;
    assert_eq!(
        places(&colliders),
        [
            (Num::ZERO, Num::ZERO),
            (Num::ONE + quarter, Num::ZERO),
            (Num::int(3) + quarter, Num::ZERO),
        ]
    );
    // A walker 1.5 m from a unit it walks into, either way round, neither parts from nor pushes
    // the other (entries-and-blocking.md, A walker passes through the unit it walks into); one
    // that walks into a third unit is near a unit 2.25 m off, in its personal space, as ever.
    for walker in [0, 1] {
        let mut pair = row(&[
            (Num::ZERO, Num::ZERO, true),
            (Num::ONE + half, Num::ZERO, true),
        ]);
        pair[walker].into = Some(pair[1 - walker].id);
        assert!(!pair[0].overlaps(&pair[1]) && !pair[0].near(&pair[1]));
        resolve(&mut pair);
        push(&mut pair, Moves::Team);
        assert_eq!(
            places(&pair),
            [(Num::ZERO, Num::ZERO), (Num::ONE + half, Num::ZERO)],
            "walker {walker}"
        );
    }
    let mut three = row(&[
        (Num::ZERO, Num::ZERO, true),
        (Num::int(2) + quarter, Num::ZERO, true),
        (Num::int(10), Num::ZERO, true),
    ]);
    three[0].into = Some(three[2].id);
    assert!(three[0].near(&three[1]));
}

#[test]
fn a_heavier_body_moves_less_and_one_that_drives_moves_only_along_its_facing() {
    // 1.5 m apart along x, radii 1: an overlap of 0.5 m. Of weights 1 and 3, the lighter takes
    // 3/4 of it, 0.375 m, and the heavier 1/4, 0.125 m.
    let x = |eighths: i64| Num::from_bits(eighths << (Num::FRAC_BITS - 3));
    let pair = || row(&[(x(0), x(0), true), (x(12), x(0), true)]);
    let mut weighed = pair();
    weighed[1].weight = Num::int(3);
    assert_eq!(resolve(&mut weighed), 0);
    assert_eq!(places(&weighed), [(x(-3), x(0)), (x(13), x(0))]);
    // The first drives. Facing +x, along the line between them, it moves as one that steps, a
    // quarter meter; facing +z, across it, it moves none, and the other takes the half meter.
    let way = |x: i128, z: i128| Direction::of([x, z]);
    let mut along = pair();
    along[0].way = way(1, 0);
    assert_eq!(resolve(&mut along), 0);
    assert_eq!(places(&along), [(x(-2), x(0)), (x(14), x(0))]);
    let mut across = pair();
    across[0].way = way(0, 1);
    assert_eq!(resolve(&mut across), 0);
    assert_eq!(places(&across), [(x(0), x(0)), (x(16), x(0))]);
    // Both drive, side by side, facing +z: no pass parts them along their facings, and the last
    // pushes them off them, a quarter meter each.
    let mut side = pair();
    for collider in &mut side {
        collider.way = way(0, 1);
    }
    assert_eq!(resolve(&mut side), 0);
    assert_eq!(places(&side), [(x(-2), x(0)), (x(14), x(0))]);
}

#[test]
fn passes_part_a_chain_within_its_slack_and_a_static_body_pins_a_walker() {
    // Three in a row 1.5 m apart, radii 1. The first pass parts the first pair, a quarter meter
    // each, to −0.25 and 1.75, which pushes the middle 0.75 m into the third, then that pair, to
    // 1.375 and 3.375: the first pair is 0.375 m deep. Each pass after quarters it, 0.375 to
    // 0.09375 in the second, so 8 passes leave 0.375 · 4⁻⁷ = 3 · 2⁻¹⁷ m, the first at
    // −0.5 + 2 · 2⁻¹⁷, the others 2⁻¹⁷ short of 1.5 and 3.5, 2 m apart: within a 64th of the
    // reach, 0.03125 m.
    let x = |bits: i64| Num::from_bits(bits << (Num::FRAC_BITS - 17));
    let meters = |meters: i64| meters << 17;
    let chain = || {
        row(&[
            (x(0), x(0), true),
            (x(meters(3) / 2), x(0), true),
            (x(meters(3)), x(0), true),
        ])
    };
    let mut parted = chain();
    assert_eq!(resolve(&mut parted), 0);
    assert_eq!(
        places(&parted),
        [
            (x(-meters(1) / 2 + 2), x(0)),
            (x(meters(3) / 2 - 1), x(0)),
            (x(meters(7) / 2 - 1), x(0)),
        ]
    );
    // A pair beside a static body, all radii 1: the first walker at 0, 0.25 m into the static
    // body at −1.75 and 0.5 m into the second walker at 1.5. The first pass parts the pair, a
    // quarter meter each, to −0.25 and 1.75, then the first from the static body, 0.5 m, to 0.25,
    // which pins it; the second pass leaves the second the whole 0.5 m, to 2.25, and the third
    // parts none.
    let mut walled = row(&[
        (x(0), x(0), true),
        (x(meters(3) / 2), x(0), true),
        (x(-meters(7) / 4), x(0), false),
    ]);
    assert_eq!(resolve(&mut walled), 0);
    assert_eq!(
        places(&walled)[..2],
        [(x(meters(1) / 4), x(0)), (x(meters(9) / 4), x(0))]
    );
}

#[test]
fn a_push_parts_a_pair_within_its_space_by_weight_and_moves_no_unit_it_may_not() {
    // Radii 1, so a space of 1.25 each, and a pair 2.25 m apart along x: 0.25 m into their 2.5 m,
    // which a push undoes a quarter of, 1/16 m, in 64ths of a meter 4, split by weight. The
    // first walks, the second stands, both of team 0.
    let x = |sixty_fourths: i64| Num::from_bits(sixty_fourths << (Num::FRAC_BITS - 6));
    let team = |team| Some(Team::new(team));
    let pair = |second: fn(&mut Collider)| {
        let mut pair = row(&[(x(0), x(0), true), (x(144), x(0), true)]);
        pair[1].walking = false;
        for collider in &mut pair {
            collider.team = team(0);
        }
        second(&mut pair[1]);
        pair
    };
    let pushed = |mut pair: Vec<Collider>, moves| {
        push(&mut pair, moves);
        places(&pair)
            .into_iter()
            .map(|(x, _)| x)
            .collect::<Vec<_>>()
    };
    // Equal weights, 2 each; the second of weight 3 takes 1, the first 3.
    assert_eq!(pushed(pair(|_| {}), Moves::Team), [x(-2), x(146)]);
    let heavy = pair(|second| second.weight = Num::int(3));
    assert_eq!(pushed(heavy, Moves::Team), [x(-3), x(145)]);
    // The second firm, or one that drives: the first takes all 4.
    let firm = pair(|second| second.firm = true);
    assert_eq!(pushed(firm, Moves::Team), [x(-4), x(144)]);
    let drives = pair(|second| second.way = Direction::of([0, 1]));
    assert_eq!(pushed(drives, Moves::Team), [x(-4), x(144)]);
    // Of another team, under a mode that moves no unit, or both standing: neither moves.
    let enemy = pair(|second| second.team = Some(Team::new(1)));
    assert_eq!(pushed(enemy, Moves::Team), [x(0), x(144)]);
    assert_eq!(pushed(pair(|_| {}), Moves::None), [x(0), x(144)]);
    let mut standing = pair(|_| {});
    standing[0].walking = false;
    assert_eq!(pushed(standing, Moves::Team), [x(0), x(144)]);
    // A static body of radius 1 at x = −130, 2 m and a 64th off the first: 2 of the push leaves
    // the first touching it, 4 would put it in, so with the second firm it takes none.
    let walled = |second: fn(&mut Collider)| {
        let mut bodies = pair(second);
        bodies.extend(row(&[(x(-130), x(0), false)]));
        bodies
    };
    let moved = |bodies: Vec<Collider>| pushed(bodies, Moves::Team)[..2].to_vec();
    assert_eq!(moved(walled(|_| {})), [x(-2), x(146)]);
    assert_eq!(moved(walled(|second| second.firm = true)), [x(0), x(144)]);
}
