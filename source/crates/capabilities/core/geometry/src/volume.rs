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
mod tests {
    use super::*;
    use crate::body_box::BodyBox;
    use crate::radius::Radius;

    #[test]
    fn a_volume_reaches_from_its_sides_top_and_bottom_in_space() {
        let num = Num::int;
        // A cylinder of radius 1 and height 2 at the origin: 2 m beside it at mid height, 3 m
        // above its top and 3 m below its bottom, and 3 m out and 4 m up from its top edge, 5 m.
        let circle = Shape::Circle(Radius::new(Num::ONE).unwrap());
        let cylinder = Volume::new(Placed::new(Vec3::ZERO, circle), num(2));
        let beside = Vec3::from((3, 1, 0));
        let above = Vec3::from((0, 5, 0));
        let below = Vec3::from((0, -3, 0));
        let corner = Vec3::from((4, 6, 0));
        for (point, distance) in [(beside, 2), (above, 3), (below, 3), (corner, 5)] {
            let at = |reach: Num| cylinder.nearest(point, reach);
            assert_eq!(at(num(distance)), Ordering::Equal, "{point:?}");
            assert_eq!(
                at(num(distance) - Num::EPSILON),
                Ordering::Greater,
                "{point:?}"
            );
            assert_eq!(
                at(num(distance) + Num::EPSILON),
                Ordering::Less,
                "{point:?}"
            );
            // A reach of exactly the distance reaches it, and one a bit short does not.
            assert!(cylinder.reaches(point, num(distance)), "{point:?}");
            assert!(
                !cylinder.reaches(point, num(distance) - Num::EPSILON),
                "{point:?}"
            );
        }
        // Its height counts: 4 m tall, the point 5 m up lies 1 m above it.
        let taller = Volume::new(cylinder.base, num(4));
        assert_eq!(taller.nearest(above, Num::ONE), Ordering::Equal);
        // Inside it is closer than every reach; on its top, at 0, as on a planar edge.
        assert_eq!(
            cylinder.nearest(Vec3::from((0, 1, 0)), Num::ZERO),
            Ordering::Less
        );
        let top = Vec3::from((0, 2, 0));
        assert_eq!(cylinder.nearest(top, Num::ZERO), Ordering::Equal);
        assert_eq!(cylinder.nearest(top, Num::EPSILON), Ordering::Less);
        // A flat volume is its shape at its base's height.
        let flat = Volume::new(cylinder.base, Num::ZERO);
        assert_eq!(flat.nearest(Vec3::from((0, 3, 0)), num(3)), Ordering::Equal);

        // A box 4 × 2 m, height 3, at (10, 1, 0): its side 2 m from its base's x. From (15, 8, 0),
        // 3 m past that side and 4 m above its top at 4 m, 5 m; from (13, 2, 0), 1 m beside it.
        let body = BodyBox::new([num(4), num(2)], Num::ZERO).unwrap();
        let boxed = Volume::new(
            Placed::new(Vec3::from((10, 1, 0)), Shape::Box(body)),
            num(3),
        );
        assert_eq!(
            boxed.nearest(Vec3::from((15, 8, 0)), num(5)),
            Ordering::Equal
        );
        assert_eq!(
            boxed.nearest(Vec3::from((15, 8, 0)), num(5) - Num::EPSILON),
            Ordering::Greater
        );
        assert_eq!(
            boxed.nearest(Vec3::from((13, 2, 0)), Num::ONE),
            Ordering::Equal
        );
        assert_eq!(
            boxed.nearest(Vec3::from((11, 2, 0)), Num::ZERO),
            Ordering::Less
        );
        // Its center is halfway up: 1 + 3 ÷ 2.
        assert_eq!(boxed.center(), Vec3::new(num(10), Num::HALF * 5, Num::ZERO));
    }
}
