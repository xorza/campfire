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

    /// The point of the shape at `pos` nearest `to` on the ground plane, at `pos`'s height: `to`
    /// itself inside; a box's nearest point, each coordinate rounded to the nearest bit; a
    /// circle's, a step of its radius from its centre toward `to`, as a walker steps.
    pub(crate) fn nearest_point(&self, pos: Position, to: Position) -> Position {
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
    pub(crate) fn comes_within(
        &self,
        pos: Position,
        from: Position,
        to: Position,
        reach: Num,
    ) -> bool {
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
            low - centre >= far || centre - high >= far
        };
        if clear(centre.x, ends.map(|end| end.x), extent[0])
            || clear(centre.z, ends.map(|end| end.z), extent[1])
        {
            return false;
        }
        match self {
            Shape::Circle(_) => {
                self.approach(Metric::Planar, pos, from, to, reach).nearest == Ordering::Less
            }
            Shape::Box(body) => body.comes_within(pos, from, to, reach),
        }
    }

    /// How the straight path from `from` to `to` comes to the shape at `pos` in `metric`, against
    /// `reach`: to a circle's centre against `reach` and its radius, a sum past every number
    /// reaching every distance, as `Metric::reaches` counts it; to a box against `reach`.
    pub(crate) fn approach(
        self,
        metric: Metric,
        pos: Position,
        from: Position,
        to: Position,
        reach: Num,
    ) -> Approach {
        match self {
            Shape::Circle(radius) => {
                let reach = reach.checked_add(radius).unwrap_or(Num::MAX);
                Approach::of(metric.offset(from, to), metric.offset(from, pos), reach)
            }
            Shape::Box(body) => {
                debug_assert!(metric == Metric::Planar, "a box lies on a planar map");
                body.approach(pos, from, to, reach)
            }
        }
    }
}
