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

    // On a 3-4-5 line: (0.3, 0.4), each a whole number of bits below, are √(5 033 164² +
    // 6 710 886²) = 8 388 607.x bits apart, the root floored to 8 388 607. The overlap of
    // 2 m less that, 25 165 825 bits, splits 12 582 912 back and 12 582 913 forward, each
    // along (5 033 164, 6 710 886) ÷ 8 388 607 and rounded once: (7 549 747, 10 066 330)
    // back and (7 549 747, 10 066 331) forward.
    let tenth = |n: i64| Num::from_bits((n << Num::FRAC_BITS) / 10);
    let mut slant = row(&[(Num::int(0), Num::int(0), true), (tenth(3), tenth(4), true)]);
    resolve(&mut slant);
    let bits = Num::from_bits;
    let expected = [
        (-bits(7_549_747), -bits(10_066_330)),
        (tenth(3) + bits(7_549_747), tenth(4) + bits(10_066_331)),
    ];
    assert_eq!(places(&slant), expected);
}
