use std::cmp::Ordering;

use campfire_math::{Num, Vec3};

use crate::approach::Approach;
use crate::polygon::Polygon;
use crate::shape::Shape;

#[cfg(feature = "internals")]
pub(crate) mod bench;

/// A body where it stands: its unit's position, and its shape around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed {
    pub position: Vec3,
    pub shape: Shape,
}

impl Placed {
    pub const fn new(position: Vec3, shape: Shape) -> Self {
        Self { position, shape }
    }

    /// A point at `position`, as a unit with no body is.
    pub const fn point(position: Vec3) -> Self {
        Self::new(position, Shape::POINT)
    }

    /// Whether the straight path from `from` to `to` on the ground plane comes closer than
    /// `reach` to it, exactly: closer than `reach` and its radius to a circle's center, or than
    /// `reach` to a box, which a path into its open inside is at any reach, 0 too, as a circle's
    /// center is. Touching at `reach` is not closer.
    pub fn comes_closer(self, from: Vec3, to: Vec3, reach: Num) -> bool {
        let Self { position, shape } = self;
        // A path whose span along an axis lies at least `reach` beyond the shape's comes no
        // closer: most shapes a search of buckets finds lie so, and take no exact test. A reach
        // past every number saturates, and no span lies beyond it.
        let (center, ends) = (position, [from, to]);
        let extent = shape.extent();
        let reach_bits = reach.to_bits();
        let clear = |center: Num, ends: [Num; 2], extent: Num| {
            let (low, high) = (
                ends[0].min(ends[1]).to_bits(),
                ends[0].max(ends[1]).to_bits(),
            );
            let (center, far) = (
                center.to_bits(),
                extent.to_bits().saturating_add(reach_bits),
            );
            // Positions lie within the world's bound, so the differences cannot wrap, and the
            // wrapping steps skip the checks a release build makes.
            low.wrapping_sub(center) >= far || center.wrapping_sub(high) >= far
        };
        if clear(center.x, ends.map(|end| end.x), extent[0])
            || clear(center.z, ends.map(|end| end.z), extent[1])
        {
            return false;
        }
        match shape {
            Shape::Circle(radius) => {
                let path = from.ground_offset(to);
                let off = from.ground_offset(position);
                Approach::nearest_of_circle(path, off, reach, radius.get()) == Ordering::Less
            }
            Shape::Box(body) => body.comes_closer(position, from, to, reach),
        }
    }

    /// Whether the insides of it and `other` share a point on the ground plane, exactly:
    /// touching is not overlap. Two circles overlap closer than the sum of their radii; a box and
    /// a circle when the circle's center lies closer than its radius to the box, which a center
    /// inside the open box does at any radius; two boxes as `BodyBox::overlaps` gives.
    pub fn overlaps(self, other: Self) -> bool {
        match (self.shape, other.shape) {
            (Shape::Circle(radius), Shape::Circle(other_radius)) => radius
                .get()
                .checked_add(other_radius.get())
                .is_none_or(|reach| {
                    Vec3::ZERO.distance_order(self.position.ground_offset(other.position), reach)
                        == Ordering::Less
                }),
            (Shape::Box(body), Shape::Circle(radius)) => {
                body.closer(self.position, other.position, radius.get())
            }
            (Shape::Circle(radius), Shape::Box(body)) => {
                body.closer(other.position, self.position, radius.get())
            }
            (Shape::Box(body), Shape::Box(other_body)) => {
                body.overlaps(self.position, &other_body, other.position)
            }
        }
    }

    /// Whether its inside and `polygon`'s share a point, exactly: touching is not overlap. A box
    /// as `BodyBox::overlaps_polygon` gives; a circle when its center lies inside the polygon,
    /// off its edge, or an edge comes closer than its radius to the center, as
    /// `Placed::comes_closer` measures a path.
    pub fn overlaps_polygon(self, polygon: &Polygon) -> bool {
        match self.shape {
            Shape::Box(body) => body.overlaps_polygon(self.position, polygon),
            Shape::Circle(radius) => {
                let points = polygon.points();
                let center = Self::point(self.position);
                let ground = |[x, z]: [Num; 2]| Vec3::new(x, self.position.y, z);
                polygon.holds_inside([self.position.x, self.position.z])
                    || (0..points.len()).any(|at| {
                        let next = points[(at + 1) % points.len()];
                        center.comes_closer(ground(points[at]), ground(next), radius.get())
                    })
            }
        }
    }
}

#[cfg(test)]
mod tests;
