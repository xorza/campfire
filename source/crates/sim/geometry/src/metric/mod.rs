use std::cmp::Ordering;

use campfire_math::{Num, Vec3};
use serde::{Deserialize, Serialize};

use crate::approach::Approach;
use crate::body_box::BodyBox;
use crate::placed::Placed;
use crate::shape::Shape;

/// How a map measures ranges, reach and sight: on the ground plane, as MOBAs and RTS games do, or
/// in 3D, as shooters and flight do.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    #[default]
    Planar,
    Spatial,
}

impl Metric {
    /// The offset from `a` to `b` that counts towards a distance: a planar map drops the height.
    pub fn offset(self, a: Vec3, b: Vec3) -> Vec3 {
        match self {
            Self::Planar => a.ground_offset(b),
            Self::Spatial => b - a,
        }
    }

    /// The unit vector along `vector` in the map's metric, which on a planar map drops the
    /// height; `None` for a vector of no length there.
    pub fn direction(self, vector: Vec3) -> Option<Vec3> {
        match self {
            Self::Planar => Vec3::new(vector.x, Num::ZERO, vector.z).normalized(),
            Self::Spatial => vector.normalized(),
        }
    }

    /// The point `step` from `from` toward `to` in the map's metric, or the point there when it
    /// is at most `step` away: a planar map steps on the ground plane, at `from`'s height.
    pub fn step_toward(self, from: Vec3, to: Vec3, step: Num) -> Vec3 {
        let to = match self {
            Self::Planar => Vec3::new(to.x, from.y, to.z),
            Self::Spatial => to,
        };
        from.step_toward(to, step)
    }

    /// Whether `range` from the edge of the body `from` reaches the edge of the body `to`: the
    /// distance at most it, as `order` gives it.
    pub fn reaches(self, from: Placed, range: Num, to: Placed) -> bool {
        self.order(from, range, to) != Ordering::Greater
    }

    /// How the distance from the edge of the body `from` to the edge of the body `to` lies against
    /// `range`, exactly, with no square root: the one primitive of reach, which ranges, areas,
    /// auras, script queries and homing hits all decide by, each pair of shapes by its own exact
    /// test (04-capabilities/00-overview.md, One reach rule). A point is a circle of radius 0. A
    /// reach past what a number holds is past every distance within the bound. A box lies only on
    /// a planar map.
    pub fn order(self, from: Placed, range: Num, to: Placed) -> Ordering {
        let (
            Placed {
                position: from,
                shape: from_shape,
            },
            Placed {
                position: to,
                shape: to_shape,
            },
        ) = (from, to);
        debug_assert!(range >= Num::ZERO);
        let to_box = |body: &BodyBox, position: Vec3, other: Vec3, radius: Num| {
            debug_assert!(self == Self::Planar, "a box lies on a planar map");
            range
                .checked_add(radius)
                .map_or(Ordering::Less, |reach| body.nearest(position, other, reach))
        };
        match (from_shape, to_shape) {
            (Shape::Circle(from_radius), Shape::Circle(to_radius)) => range
                .checked_add(from_radius.get())
                .and_then(|reach| reach.checked_add(to_radius.get()))
                .map_or(Ordering::Less, |reach| {
                    Vec3::ZERO.distance_order(self.offset(from, to), reach)
                }),
            (Shape::Circle(radius), Shape::Box(body)) => to_box(&body, to, from, radius.get()),
            (Shape::Box(body), Shape::Circle(radius)) => to_box(&body, from, to, radius.get()),
            (Shape::Box(body), Shape::Box(other)) => {
                debug_assert!(self == Self::Planar, "a box lies on a planar map");
                body.nearest_box(from, &other, to, range)
            }
        }
    }

    /// How the straight path from `from` to `to` comes to `body`, against `reach`, as `reaches`
    /// counts it.
    pub(crate) fn approach(self, body: Placed, from: Vec3, to: Vec3, reach: Num) -> Approach {
        let Placed { position, shape } = body;
        match shape {
            Shape::Circle(radius) => Approach::of_circle(
                self.offset(from, to),
                self.offset(from, position),
                reach,
                radius.get(),
            ),
            Shape::Box(body) => {
                debug_assert!(self == Self::Planar, "a box lies on a planar map");
                body.approach(position, from, to, reach)
            }
        }
    }

    /// Whether the straight path from `from` to `to` comes within `reach` of `body`, each
    /// distance to the nearest bit; and if it does, the place along the path of its point nearest
    /// the body, the first where it enters a box, its distance from `from`.
    pub fn meets(self, from: Vec3, to: Vec3, body: Placed, reach: Num) -> Option<Num> {
        let approach = self.approach(body, from, to, reach);
        (approach.nearest != Ordering::Greater).then_some(approach.place)
    }
}

#[cfg(test)]
mod tests;
