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
mod tests;
