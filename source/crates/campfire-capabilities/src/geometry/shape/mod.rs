use std::cmp::Ordering;

use campfire_math::{Num, Vec3};
use campfire_sim::Position;
use serde::{Deserialize, Serialize};

use crate::geometry::approach::Approach;
use crate::geometry::body_box::BodyBox;
use crate::geometry::metric::Metric;

#[cfg(feature = "bench")]
pub(crate) mod bench;

/// A body's shape on the ground plane around its unit's position: a circle of a radius, 0 for
/// a point or a unit with no body, or a box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Shape {
    Circle(Num),
    Box(BodyBox),
}

impl Shape {
    pub(crate) const POINT: Shape = Shape::Circle(Num::ZERO);

    /// The widest a body's bound may be, a circle's radius or the farthest corner of a box, 2³⁵
    /// bits: the power of two past Zero Hour's widest object, and small enough that two bodies'
    /// bounds and a range add up within a `Num`.
    pub(crate) const MAX_BOUND: Num = Num::int(2048);

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

    /// The point of the shape at `at` nearest `to` on the ground plane, at `at`'s height: `to`
    /// itself inside; a box's nearest point, each coordinate rounded to the nearest bit; a
    /// circle's, a step of its radius from its centre toward `to`, as a walker steps.
    pub(crate) fn nearest_point(&self, at: Position, to: Position) -> Position {
        let ground = Position::new(Vec3::new(to.get().x, at.get().y, to.get().z))
            .expect("a point within the bound");
        match self {
            Shape::Box(body) => body.nearest_point(at, ground),
            Shape::Circle(radius) if at.within_ground(ground, *radius) => ground,
            Shape::Circle(radius) => {
                let step = at.get().step_toward(ground.get(), *radius);
                Position::new(step).expect("a step within a body's radius stays within the bound")
            }
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
        self.approach(Metric::Planar, at, from, to, reach).nearest == Ordering::Less
    }

    /// How the straight path from `from` to `to` comes to the shape at `at` in `metric`, against
    /// `reach`: to a circle's centre against `reach` and its radius, a sum past every number
    /// reaching every distance, as `Metric::reaches` counts it; to a box against `reach`.
    pub(crate) fn approach(
        self,
        metric: Metric,
        at: Position,
        from: Position,
        to: Position,
        reach: Num,
    ) -> Approach {
        match self {
            Shape::Circle(radius) => {
                let reach = reach.checked_add(radius).unwrap_or(Num::MAX);
                Approach::of(metric.offset(from, to), metric.offset(from, at), reach)
            }
            Shape::Box(body) => {
                debug_assert!(metric == Metric::Planar, "a box lies on a planar map");
                body.approach(at, from, to, reach)
            }
        }
    }
}
