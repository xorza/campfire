use bevy_ecs::entity::Entity;
use campfire_math::{Num, Vec3};
use campfire_sim::StableId;

/// A living unit's body as collision sees it: where it stands, its radius, and whether it may be
/// pushed. A unit that does not walk, such as a tower, is never pushed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Collider {
    pub(crate) id: StableId,
    pub(crate) entity: Entity,
    pub(crate) at: Vec3,
    pub(crate) radius: Num,
    pub(crate) movable: bool,
}

impl Collider {
    /// Pushes each overlapping pair of `colliders`, sorted by stable id, apart along the line
    /// between them, on the ground plane, in the order of their ids: each pair sees the pushes of
    /// the pairs before it. Two that may both be pushed share the overlap, the higher id taking
    /// the odd bit; one that may not be pushed leaves the whole overlap to the other. Two on one
    /// spot part along x, the higher id towards +x. A pair that only touches does not overlap.
    pub(crate) fn resolve(colliders: &mut [Collider]) {
        debug_assert!(colliders.is_sorted_by_key(|collider| collider.id));
        for first in 0..colliders.len() {
            for second in first + 1..colliders.len() {
                let (head, tail) = colliders.split_at_mut(second);
                Collider::part(&mut head[first], &mut tail[0]);
            }
        }
    }

    fn part(a: &mut Collider, b: &mut Collider) {
        if !a.movable && !b.movable {
            return;
        }
        // In bits of a `Num`: a position and a radius are within 2⁴⁵ bits, so squares fit i128.
        let dx = i128::from(b.at.x.to_bits() - a.at.x.to_bits());
        let dz = i128::from(b.at.z.to_bits() - a.at.z.to_bits());
        let reach = i128::from(a.radius.to_bits() + b.radius.to_bits());
        let square = dx * dx + dz * dz;
        if square >= reach * reach {
            return;
        }
        let distance = square.cast_unsigned().isqrt().cast_signed();
        let overlap = reach - distance;
        let (dx, dz, distance) = if distance == 0 {
            (1, 0, 1)
        } else {
            (dx, dz, distance)
        };
        let (back, forward) = match (a.movable, b.movable) {
            (true, true) => (overlap / 2, overlap - overlap / 2),
            (true, false) => (overlap, 0),
            _ => (0, overlap),
        };
        a.at = Collider::shift(a.at, -back, dx, dz, distance);
        b.at = Collider::shift(b.at, forward, dx, dz, distance);
    }

    /// `at` moved `amount` bits along the direction `(dx, dz)` of length `distance` bits, each
    /// component rounded to the nearest bit, half away from zero; its height stays.
    fn shift(at: Vec3, amount: i128, dx: i128, dz: i128, distance: i128) -> Vec3 {
        let step = |along: i128| {
            let exact = amount * along;
            let rounded = (exact.abs() + distance / 2) / distance * exact.signum();
            i64::try_from(rounded).expect("a push is shorter than two radii")
        };
        Vec3::new(
            Num::from_bits(at.x.to_bits() + step(dx)),
            at.y,
            Num::from_bits(at.z.to_bits() + step(dz)),
        )
    }
}

#[cfg(test)]
mod tests {
    use bevy_ecs::world::World;
    use campfire_sim::IdAllocator;

    use super::*;

    fn num(value: i64) -> Num {
        Num::from_int(value).unwrap()
    }

    /// Colliders of radius 1 at each x on the ground, in id order, each movable as given.
    fn row(at: &[(Num, Num, bool)]) -> Vec<Collider> {
        let mut ids = IdAllocator::default();
        let mut world = World::new();
        at.iter()
            .map(|&(x, z, movable)| Collider {
                id: ids.allocate(),
                entity: world.spawn_empty().id(),
                at: Vec3::new(x, num(2), z),
                radius: Num::ONE,
                movable,
            })
            .collect()
    }

    fn places(colliders: &[Collider]) -> Vec<(Num, Num)> {
        colliders.iter().map(|c| (c.at.x, c.at.z)).collect()
    }

    #[test]
    fn overlapping_bodies_part_to_the_sum_of_their_radii() {
        // 1.5 m apart, radii 1 and 1: an overlap of 0.5, a quarter meter each way.
        let half = Num::from_bits(1 << 23);
        let quarter = Num::from_bits(1 << 22);
        let mut two = row(&[(num(0), num(0), true), (num(1) + half, num(0), true)]);
        Collider::resolve(&mut two);
        assert_eq!(
            places(&two),
            [(-quarter, num(0)), (num(1) + half + quarter, num(0))]
        );
        assert_eq!(two[0].at.y, num(2));

        // A tower that may not be pushed leaves the whole half meter to the walker.
        let mut tower = row(&[(num(0), num(0), false), (num(0), num(1) + half, true)]);
        Collider::resolve(&mut tower);
        assert_eq!(places(&tower), [(num(0), num(0)), (num(0), num(2))]);

        // Touching, 2 m apart, is no overlap; two towers never move.
        for (x, movable) in [(num(2), true), (num(1), false)] {
            let mut pair = row(&[(num(0), num(0), movable), (x, num(0), movable)]);
            let before = places(&pair);
            Collider::resolve(&mut pair);
            assert_eq!(places(&pair), before);
        }

        // On one spot they part along x by a meter each, the higher id towards +x.
        let mut stacked = row(&[(num(3), num(3), true), (num(3), num(3), true)]);
        Collider::resolve(&mut stacked);
        assert_eq!(places(&stacked), [(num(2), num(3)), (num(4), num(3))]);

        // On a 3-4-5 line: (0.3, 0.4), each a whole number of bits below, are √(5 033 164² +
        // 6 710 886²) = 8 388 607.x bits apart, the root floored to 8 388 607. The overlap of
        // 2 m less that, 25 165 825 bits, splits 12 582 912 back and 12 582 913 forward, each
        // along (5 033 164, 6 710 886) ÷ 8 388 607 and rounded once: (7 549 747, 10 066 330)
        // back and (7 549 747, 10 066 331) forward.
        let tenth = |n: i64| Num::from_bits((n << Num::FRAC_BITS) / 10);
        let mut slant = row(&[(num(0), num(0), true), (tenth(3), tenth(4), true)]);
        Collider::resolve(&mut slant);
        let bits = Num::from_bits;
        let expected = [
            (-bits(7_549_747), -bits(10_066_330)),
            (tenth(3) + bits(7_549_747), tenth(4) + bits(10_066_331)),
        ];
        assert_eq!(places(&slant), expected);
    }
}
