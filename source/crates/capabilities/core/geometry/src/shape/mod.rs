use std::cmp::Ordering;

use campfire_math::{Num, Vec3};
use campfire_sim::Position;
use serde::{Deserialize, Serialize};

use crate::approach::Approach;
use crate::body_box::BodyBox;

#[cfg(feature = "internals")]
pub(crate) mod bench;

/// A body's shape on the ground plane around its unit's position: a circle of a radius, 0 for
/// a point or a unit with no body, or a box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Shape {
    Circle(Num),
    Box(BodyBox),
}

impl Shape {
    pub const POINT: Shape = Shape::Circle(Num::ZERO);

    /// The radius of the least circle round the position that holds it.
    pub const fn bound(&self) -> Num {
        match self {
            Shape::Circle(radius) => *radius,
            Shape::Box(body) => body.bound(),
        }
    }

    /// How far it reaches from the position along x and along z.
    pub fn extent(&self) -> [Num; 2] {
        match self {
            Shape::Circle(radius) => [*radius; 2],
            Shape::Box(body) => body.extent(),
        }
    }

    /// The point of the shape at `pos` nearest `to` on the ground plane, at `pos`'s height: `to`
    /// itself inside; a box's nearest point, each coordinate rounded to the nearest bit; a
    /// circle's, a step of its radius from its centre toward `to`, as a walker steps.
    pub fn nearest_point(&self, pos: Position, to: Position) -> Position {
        let ground = Position::new(Vec3::new(to.get().x, pos.get().y, to.get().z))
            .expect("a point within the bound");
        match self {
            Shape::Box(body) => body.nearest_point(pos, ground),
            Shape::Circle(radius) if pos.within_ground(ground, *radius) => ground,
            Shape::Circle(radius) => {
                let step = pos.get().step_toward(ground.get(), *radius);
                Position::new(step).expect("a step within a body's radius stays within the bound")
            }
        }
    }

    /// Whether the straight path from `from` to `to` on the ground plane comes closer than
    /// `reach` to the shape at `pos`, exactly: closer than `reach` and its radius to a circle's
    /// centre, or than `reach` to a box. Touching at `reach` is not closer.
    pub fn comes_within(&self, pos: Position, from: Position, to: Position, reach: Num) -> bool {
        // A path whose span along an axis lies at least `reach` beyond the shape's comes no
        // closer: most shapes a search of buckets finds lie so, and take no exact test. A reach
        // past every number saturates, and no span lies beyond it.
        let (centre, ends) = (pos.get(), [from.get(), to.get()]);
        let extent = self.extent();
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
        match self {
            Shape::Circle(radius) => {
                let path = from.ground_offset(to);
                let off = from.ground_offset(pos);
                Approach::nearest_of_circle(path, off, reach, *radius) == Ordering::Less
            }
            Shape::Box(body) => body.comes_within(pos, from, to, reach),
        }
    }
}
