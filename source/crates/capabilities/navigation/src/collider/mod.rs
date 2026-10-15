use bevy_ecs::entity::Entity;
use campfire_geometry::{BodyBox, Shape};
use campfire_math::{Direction, Num, Vec3};
use campfire_sim::StableId;
use campfire_units::Layer;

use crate::collider::contact::Contact;

pub(crate) mod contact;

/// A living unit's body as collision sees it: where it stands, its shape, its layer, whether it
/// may be pushed, whether it walks now, to a destination, and whether it gathers, as a worker in
/// its gather loop, which passes through another. A unit that cannot walk, such as a tower or a
/// building, is never pushed; only such a unit has a box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Collider {
    pub id: StableId,
    pub(crate) entity: Entity,
    pub(crate) point: Vec3,
    pub shape: Shape,
    pub(crate) layer: Layer,
    pub(crate) movable: bool,
    pub(crate) walking: bool,
    pub(crate) gathering: bool,
}

/// How one body lies from another that it overlaps: the offset to it along x and z, and the two
/// radii together, in bits of a `Num`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Overlap {
    dx: i128,
    dz: i128,
    reach: i128,
}

impl Collider {
    /// Pushes the two colliders of each of `contacts`, in order, apart along the line between
    /// them, on the ground plane, until their centres lie the sum of their radii apart, each
    /// coordinate of that offset rounded away from zero past the direction's own rounding, so
    /// they end touching or apart, less than a bit past it on each axis: each contact sees the
    /// pushes of the contacts before it, and one they parted already is skipped. A unit that
    /// walks into one that stands takes the whole overlap, so no unit shoves another aside; two
    /// that both walk, or both stand, share it, the higher id taking the odd bit of each
    /// coordinate; one that may not be pushed leaves the whole overlap to the other. Two on one
    /// spot part along x, the higher id towards +x.
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
    /// of other layers, nor two that both gather.
    pub(crate) fn overlaps(&self, other: &Collider) -> bool {
        if !self.may_part(other) {
            return false;
        }
        match Collider::boxed(self, other) {
            Some((body, at, mover)) => body.push_out(at, mover.point, mover.radius()).is_some(),
            None => self.overlap(other).is_some(),
        }
    }

    /// Whether the two may part: they stand on one layer, one of them may be pushed, and not both
    /// gather.
    fn may_part(&self, other: &Collider) -> bool {
        self.layer == other.layer
            && (self.movable || other.movable)
            && !(self.gathering && other.gathering)
    }

    /// The radius of a body that may be pushed, which is a circle.
    pub fn radius(&self) -> Num {
        match self.shape {
            Shape::Circle(radius) => radius.get(),
            Shape::Box(_) => panic!("a box is never pushed, so it has no radius to reach by"),
        }
    }

    /// The box of the two, where it stands, and the other, which may be pushed: none when
    /// neither is a box. Only a body that may not be pushed is one.
    const fn boxed<'a>(a: &'a Collider, b: &'a Collider) -> Option<(BodyBox, Vec3, &'a Collider)> {
        let (body, at, mover) = match (a.shape, b.shape) {
            (Shape::Box(body), _) => (body, a.point, b),
            (_, Shape::Box(body)) => (body, b.point, a),
            _ => return None,
        };
        Some((body, at, mover))
    }

    /// How `other` lies from `self`, two circles, when the two overlap and may part, in bits of a
    /// `Num`: a position and a radius are within 2⁴⁵ bits, so squares fit i128.
    fn overlap(&self, other: &Collider) -> Option<Overlap> {
        if !self.may_part(other) {
            return None;
        }
        let dx = i128::from(other.point.x.to_bits() - self.point.x.to_bits());
        let dz = i128::from(other.point.z.to_bits() - self.point.z.to_bits());
        let reach = i128::from(self.shape.bound().to_bits() + other.shape.bound().to_bits());
        let square = dx * dx + dz * dz;
        (square < reach * reach).then_some(Overlap { dx, dz, reach })
    }

    fn part(a: &mut Collider, b: &mut Collider) {
        if !a.may_part(b) {
            return;
        }
        if let Some((body, at, _)) = Collider::boxed(a, b) {
            let mover = if b.movable { b } else { a };
            if let Some(moved) = body.push_out(at, mover.point, mover.radius()) {
                mover.point = moved;
            }
            return;
        }
        let Some(Overlap { dx, dz, reach }) = a.overlap(b) else {
            return;
        };
        let change = match Direction::of([dx, dz]) {
            None => [reach, 0],
            Some(direction) => {
                let parted = direction.scaled_away(reach);
                [0, 1].map(|axis| parted[axis] - [dx, dz][axis])
            }
        };
        let moves = match (a.movable, b.movable) {
            (true, true) if a.walking != b.walking => (a.walking, b.walking),
            pair => pair,
        };
        let back = change.map(|along| match moves {
            (true, true) => along / 2,
            (true, false) => along,
            _ => 0,
        });
        a.point = Collider::moved(a.point, back.map(|along| -along));
        b.point = Collider::moved(b.point, [0, 1].map(|axis| change[axis] - back[axis]));
    }

    /// `point` moved by `by` bits along x and z; its height stays.
    fn moved(point: Vec3, by: [i128; 2]) -> Vec3 {
        let bits = |along: i128| i64::try_from(along).expect("a push is shorter than two radii");
        Vec3::new(
            Num::from_bits(point.x.to_bits() + bits(by[0])),
            point.y,
            Num::from_bits(point.z.to_bits() + bits(by[1])),
        )
    }
}

#[cfg(test)]
mod tests;
