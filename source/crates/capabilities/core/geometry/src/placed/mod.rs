use std::cmp::Ordering;

use campfire_math::Num;
use campfire_sim::Position;

use crate::approach::Approach;
use crate::shape::Shape;

#[cfg(feature = "internals")]
pub(crate) mod bench;

/// A body where it stands: its unit's position, and its shape around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed {
    pub pos: Position,
    pub shape: Shape,
}

impl Placed {
    pub const fn new(pos: Position, shape: Shape) -> Placed {
        Placed { pos, shape }
    }

    /// A point at `pos`, as a unit with no body is.
    pub const fn point(pos: Position) -> Placed {
        Placed::new(pos, Shape::POINT)
    }

    /// Whether the straight path from `from` to `to` on the ground plane comes closer than
    /// `reach` to it, exactly: closer than `reach` and its radius to a circle's centre, or than
    /// `reach` to a box. Touching at `reach` is not closer.
    pub fn comes_closer(self, from: Position, to: Position, reach: Num) -> bool {
        let Placed { pos, shape } = self;
        // A path whose span along an axis lies at least `reach` beyond the shape's comes no
        // closer: most shapes a search of buckets finds lie so, and take no exact test. A reach
        // past every number saturates, and no span lies beyond it.
        let (centre, ends) = (pos.get(), [from.get(), to.get()]);
        let extent = shape.extent();
        let reach_bits = reach.to_bits();
        let clear = |centre: Num, ends: [Num; 2], extent: Num| {
            let (low, high) = (
                ends[0].min(ends[1]).to_bits(),
                ends[0].max(ends[1]).to_bits(),
            );
            let (centre, far) = (
                centre.to_bits(),
                extent.to_bits().saturating_add(reach_bits),
            );
            // Positions lie within the world's bound, so the differences cannot wrap, and the
            // wrapping steps skip the checks a release build makes.
            low.wrapping_sub(centre) >= far || centre.wrapping_sub(high) >= far
        };
        if clear(centre.x, ends.map(|end| end.x), extent[0])
            || clear(centre.z, ends.map(|end| end.z), extent[1])
        {
            return false;
        }
        match shape {
            Shape::Circle(radius) => {
                let path = from.ground_offset(to);
                let off = from.ground_offset(pos);
                Approach::nearest_of_circle(path, off, reach, radius) == Ordering::Less
            }
            Shape::Box(body) => body.comes_closer(pos, from, to, reach),
        }
    }
}
