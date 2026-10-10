use std::array;

use bevy_ecs::world::World;
use campfire_sim::IdAllocator;

use super::*;
use crate::navigation::broadphase::Broadphase;
use crate::navigation::broadphase::internals::statics;
/// Colliders of radius 1 at each place on the ground, in id order, each movable as given, and
/// walking when movable.
fn row(at: &[(Num, Num, bool)]) -> Vec<Collider> {
    let mut ids = IdAllocator::default();
    let mut world = World::new();
    at.iter()
        .map(|&(x, z, movable)| Collider {
            id: ids.allocate(),
            entity: world.spawn_empty().id(),
            at: Vec3::new(x, Num::int(2), z),
            shape: Shape::Circle(Num::ONE),
            layer: Layer::FIRST,
            movable,
            walking: movable,
            gathering: false,
        })
        .collect()
}

/// Parts the bodies of `colliders` that overlap, as the Collide stage does.
fn resolve(colliders: &mut [Collider]) {
    let index = statics(colliders);
    let contacts = Broadphase::default().contacts(colliders, &index).to_vec();
    Collider::resolve(colliders, &contacts);
}

fn places(colliders: &[Collider]) -> Vec<(Num, Num)> {
    colliders.iter().map(|c| (c.at.x, c.at.z)).collect()
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
    assert_eq!(two[0].at.y, Num::int(2));

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

    // Centres 1 bit apart along each axis are √2 bits apart. That offset made 33 554 432 bits
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

#[test]
fn parted_bodies_lie_the_sum_of_their_radii_apart_rounded_up_to_a_bit() {
    // Offsets in bits from the first body to the second, with their radii in meters. The
    // squared lengths 2, 74, 2⁴⁹ + 2²⁵ + 1 and 2⁷⁰ + 24 690·2³⁵ + 12 345² + 2⁶⁸ are not
    // squares; 81, 25·2⁴⁰ and 1 are. The last four pairs stand at `Shape::MAX_BOUND`, 2,048 m,
    // where the product of an offset and the reach squares past `u128`.
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
            collider.shape = Shape::Circle(Num::int(radius));
        }
        let before: [[i64; 2]; 2] =
            array::from_fn(|i| [pair[i].at.x, pair[i].at.z].map(Num::to_bits));
        resolve(&mut pair);
        let moved: [[i128; 2]; 2] = array::from_fn(|i| {
            let after = [pair[i].at.x, pair[i].at.z].map(Num::to_bits);
            [0, 1].map(|axis| i128::from(after[axis] - before[i][axis]))
        });
        let reach = i128::from((radii[0] + radii[1]) << Num::FRAC_BITS);
        let along = offset.map(i128::from);
        let square = along[0] * along[0] + along[1] * along[1];
        let parted = [0, 1].map(|axis| along[axis] + moved[1][axis] - moved[0][axis]);
        for axis in 0..2 {
            // `n` is the exact |along| · reach ÷ √square rounded up when
            // (n − 1)² · square < along² · reach² ≤ n² · square, or both are 0.
            let n = parted[axis].unsigned_abs();
            let product = (along[axis] * reach).unsigned_abs();
            let exact = U256::product(product, product);
            let square = square.cast_unsigned();
            let case = format!("{offset:?} with radii {radii:?}, axis {axis}");
            assert_eq!(parted[axis].signum(), along[axis].signum(), "{case}");
            assert!(exact <= U256::product(n.pow(2), square), "{case}");
            assert!(
                n == 0 || U256::product((n - 1).pow(2), square) < exact,
                "{case}"
            );
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
fn two_gatherers_pass_through_each_other_and_part_from_any_other() {
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
}
