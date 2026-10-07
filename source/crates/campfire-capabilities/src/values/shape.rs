use std::cmp::Ordering;

use campfire_math::Num;
use campfire_sim::Position;
use serde::{Deserialize, Serialize};

use crate::values::body_box::BodyBox;
use crate::values::metric::{Approach, Metric};

/// A body's shape on the ground plane around its unit's position: a circle of a radius, 0 for
/// a point or a unit with no body, or a box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Shape {
    Circle(Num),
    Box(BodyBox),
}

impl Shape {
    pub(crate) const POINT: Shape = Shape::Circle(Num::ZERO);

    /// The radius of the least circle round the position that holds it.
    pub(crate) const fn bound(&self) -> Num {
        match self {
            Shape::Circle(radius) => *radius,
            Shape::Box(body) => body.bound(),
        }
    }

    /// How far it reaches from the position along x and along z.
    pub(crate) fn extent(&self) -> [Num; 2] {
        match self {
            Shape::Circle(radius) => [*radius; 2],
            Shape::Box(body) => body.extent(),
        }
    }

    /// Whether the straight path from `from` to `to` on the ground plane comes closer than
    /// `reach` to the shape at `at`, exactly: closer than `reach` and its radius to a circle's
    /// centre, or than `reach` to a box. Touching at `reach` is not closer.
    pub(crate) fn comes_within(
        &self,
        at: Position,
        from: Position,
        to: Position,
        reach: Num,
    ) -> bool {
        match self {
            Shape::Circle(radius) => {
                let path = Metric::Planar.offset(from, to);
                let off = Metric::Planar.offset(from, at);
                Approach::of(path, off, reach + *radius).nearest == Ordering::Less
            }
            Shape::Box(body) => body.approach(at, from, to, reach).nearest == Ordering::Less,
        }
    }
}
