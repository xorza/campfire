use bevy_ecs::entity::Entity;
use campfire_math::{Num, Vec3};
use campfire_sim::StableId;

use crate::navigation::broadphase::Contact;
use crate::units::layer::Layer;

/// A living unit's body as collision sees it: where it stands, its radius, its layer, whether it
/// may be pushed, and whether it walks now, to a destination. A unit that cannot walk, such as a
/// tower, is never pushed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Collider {
    pub(crate) id: StableId,
    pub(crate) entity: Entity,
    pub(crate) at: Vec3,
    pub(crate) radius: Num,
    pub(crate) layer: Layer,
    pub(crate) movable: bool,
    pub(crate) walking: bool,
}

/// How one body lies from another that it overlaps: the offset to it along x and z, its square,
/// and the two radii together, in bits of a `Num`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Overlap {
    dx: i128,
    dz: i128,
    square: i128,
    reach: i128,
}

impl Collider {
    /// Pushes the two colliders of each of `contacts`, in order, apart along the line between
    /// them, on the ground plane: each contact sees the pushes of the contacts before it, and one
    /// they parted already is skipped. A unit that walks into one that stands takes the whole
    /// overlap, so no unit shoves another aside; two that both walk, or both stand, share it, the
    /// higher id taking the odd bit; one that may not be pushed leaves the whole overlap to the
    /// other. Two on one spot part along x, the higher id towards +x.
    pub(crate) fn resolve(colliders: &mut [Collider], contacts: &[Contact]) {
        debug_assert!(colliders.is_sorted_by_key(|collider| collider.id));
        for &Contact { first, second } in contacts {
            debug_assert!(first < second);
            let (head, tail) = colliders.split_at_mut(second);
            Collider::part(&mut head[first], &mut tail[0]);
        }
    }

    /// Whether the bodies of `self` and `other` overlap on the ground plane, exactly: touching is
    /// not overlap. Two that may not be pushed never part, so they have no contact, nor have two
    /// of other layers.
    pub(crate) fn overlaps(&self, other: &Collider) -> bool {
        self.overlap(other).is_some()
    }

    /// How `other` lies from `self`, when the two overlap and may part, in bits of a `Num`: a
    /// position and a radius are within 2⁴⁵ bits, so squares fit i128.
    fn overlap(&self, other: &Collider) -> Option<Overlap> {
        if !self.movable && !other.movable || self.layer != other.layer {
            return None;
        }
        let dx = i128::from(other.at.x.to_bits() - self.at.x.to_bits());
        let dz = i128::from(other.at.z.to_bits() - self.at.z.to_bits());
        let reach = i128::from(self.radius.to_bits() + other.radius.to_bits());
        let square = dx * dx + dz * dz;
        (square < reach * reach).then_some(Overlap {
            dx,
            dz,
            square,
            reach,
        })
    }

    fn part(a: &mut Collider, b: &mut Collider) {
        let Some(Overlap {
            dx,
            dz,
            square,
            reach,
        }) = a.overlap(b)
        else {
            return;
        };
        let distance = square.cast_unsigned().isqrt().cast_signed();
        let overlap = reach - distance;
        let (dx, dz, distance) = if distance == 0 {
            (1, 0, 1)
        } else {
            (dx, dz, distance)
        };
        let moves = match (a.movable, b.movable) {
            (true, true) if a.walking != b.walking => (a.walking, b.walking),
            pair => pair,
        };
        let (back, forward) = match moves {
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
    use crate::navigation::broadphase::Broadphase;
    use crate::navigation::broadphase::internals::statics;

    fn num(value: i64) -> Num {
        Num::from_int(value).unwrap()
    }

    /// Colliders of radius 1 at each place on the ground, in id order, each movable as given, and
    /// walking when movable.
    fn row(at: &[(Num, Num, bool)]) -> Vec<Collider> {
        let mut ids = IdAllocator::default();
        let mut world = World::new();
        at.iter()
            .map(|&(x, z, movable)| Collider {
                id: ids.allocate(),
                entity: world.spawn_empty().id(),
                at: Vec3::new(x, num(2), z),
                radius: Num::ONE,
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
        let half = Num::from_bits(1 << 23);
        let quarter = Num::from_bits(1 << 22);
        let mut two = row(&[(num(0), num(0), true), (num(1) + half, num(0), true)]);
        resolve(&mut two);
        assert_eq!(
            places(&two),
            [(-quarter, num(0)), (num(1) + half + quarter, num(0))]
        );
        assert_eq!(two[0].at.y, num(2));

        // A unit that walks into one that stands takes the whole half meter, whichever id it has:
        // no unit shoves another aside.
        for walker in [0, 1] {
            let mut pair = row(&[(num(0), num(0), true), (num(1) + half, num(0), true)]);
            pair[1 - walker].walking = false;
            resolve(&mut pair);
            let expected = [
                [(-half, num(0)), (num(1) + half, num(0))],
                [(num(0), num(0)), (num(2), num(0))],
            ];
            assert_eq!(places(&pair), expected[walker], "walker {walker}");
        }

        // A tower that may not be pushed leaves the whole half meter to the walker.
        let mut tower = row(&[(num(0), num(0), false), (num(0), num(1) + half, true)]);
        resolve(&mut tower);
        assert_eq!(places(&tower), [(num(0), num(0)), (num(0), num(2))]);

        // Touching, 2 m apart, is no overlap; two towers never move.
        for (x, movable) in [(num(2), true), (num(1), false)] {
            let mut pair = row(&[(num(0), num(0), movable), (x, num(0), movable)]);
            let before = places(&pair);
            resolve(&mut pair);
            assert_eq!(places(&pair), before);
        }

        // On one spot they part along x by a meter each, the higher id towards +x.
        let mut stacked = row(&[(num(3), num(3), true), (num(3), num(3), true)]);
        resolve(&mut stacked);
        assert_eq!(places(&stacked), [(num(2), num(3)), (num(4), num(3))]);

        // On a 3-4-5 line: (0.3, 0.4), each a whole number of bits below, are √(5 033 164² +
        // 6 710 886²) = 8 388 607.x bits apart, the root floored to 8 388 607. The overlap of
        // 2 m less that, 25 165 825 bits, splits 12 582 912 back and 12 582 913 forward, each
        // along (5 033 164, 6 710 886) ÷ 8 388 607 and rounded once: (7 549 747, 10 066 330)
        // back and (7 549 747, 10 066 331) forward.
        let tenth = |n: i64| Num::from_bits((n << Num::FRAC_BITS) / 10);
        let mut slant = row(&[(num(0), num(0), true), (tenth(3), tenth(4), true)]);
        resolve(&mut slant);
        let bits = Num::from_bits;
        let expected = [
            (-bits(7_549_747), -bits(10_066_330)),
            (tenth(3) + bits(7_549_747), tenth(4) + bits(10_066_331)),
        ];
        assert_eq!(places(&slant), expected);
    }
}
