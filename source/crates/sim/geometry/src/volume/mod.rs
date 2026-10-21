use std::cmp::Ordering;

use campfire_math::{Num, RoundedRoot, Vec3};

use crate::placed::Placed;
use crate::shape::Shape;

/// A body's volume: its shape on the ground plane around its base, from the base's height up by its
/// height, a circle's a cylinder and a box's a box (04-capabilities/00-overview.md, A body has a
/// height).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Volume {
    pub base: Placed,
    pub height: Num,
}

impl Volume {
    pub fn new(base: Placed, height: Num) -> Self {
        debug_assert!(height >= Num::ZERO, "a volume's height is at least 0");
        Self { base, height }
    }

    /// Its center: its base raised by half its height, to the nearest bit.
    pub fn center(&self) -> Vec3 {
        let Placed { position, .. } = self.base;
        Vec3::new(position.x, position.y + self.height / 2, position.z)
    }

    /// Whether `reach` from `point` reaches it in space: the distance at most it, as `nearest`
    /// gives it (04-capabilities/00-overview.md, One reach rule).
    pub fn reaches(&self, point: Vec3, reach: Num) -> bool {
        self.nearest(point, reach) != Ordering::Greater
    }

    /// How the distance in space from `point` to it lies against `reach`, at least 0: a point
    /// inside it is closer than every reach. Within its heights, as the planar reach rule
    /// measures, exactly; above or below them, from its gap on the ground plane, to the nearest
    /// bit, and the gap in height, their squares exactly (07-determinism-core.md, Exact where it
    /// fits).
    fn nearest(&self, point: Vec3, reach: Num) -> Ordering {
        debug_assert!(reach >= Num::ZERO, "a reach is at least 0");
        let Placed { position, shape } = self.base;
        let [low, high, at] =
            [position.y, position.y + self.height, point.y].map(|y| i128::from(y.to_bits()));
        let rise = (low - at).max(at - high).max(0);
        let ground = || position.ground_offset(point).length_squared_bits();
        if rise == 0 {
            let beside = match shape {
                Shape::Circle(radius) => {
                    radius
                        .get()
                        .checked_add(reach)
                        .map_or(Ordering::Less, |total| {
                            let total = u128::from(total.to_bits().unsigned_abs());
                            ground().cmp(&(total * total))
                        })
                }
                Shape::Box(body) => body.nearest(position, point, reach),
            };
            // On its top or its bottom a point inside its shape lies on it, not inside it.
            let on_face = at == low || at == high;
            return if on_face && reach == Num::ZERO && beside == Ordering::Less {
                Ordering::Equal
            } else {
                beside
            };
        }
        let gap = match shape {
            Shape::Circle(radius) => {
                let center = ground().nearest_root().cast_signed();
                (center - i128::from(radius.get().to_bits())).max(0)
            }
            Shape::Box(body) => body.gap(position, point),
        };
        let square = |bits: i128| bits.unsigned_abs() * bits.unsigned_abs();
        let reach = u128::from(reach.to_bits().unsigned_abs());
        (square(gap) + square(rise)).cmp(&(reach * reach))
    }
}

#[cfg(test)]
mod tests;
