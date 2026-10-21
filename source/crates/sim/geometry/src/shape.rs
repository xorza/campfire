use campfire_math::{Direction, Num, Vec3};
use serde::{Deserialize, Serialize};

use crate::body_box::BodyBox;
use crate::radius::Radius;

/// A body's shape on the ground plane around its unit's position: a circle of a radius, 0 for
/// a point or a unit with no body, or a box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Shape {
    Circle(Radius),
    Box(BodyBox),
}

impl Shape {
    pub const POINT: Self = Self::Circle(Radius::ZERO);

    /// The radius of the least circle round the position that holds it.
    pub const fn bound(&self) -> Num {
        match self {
            Self::Circle(radius) => radius.get(),
            Self::Box(body) => body.bound(),
        }
    }

    /// It grown by `by` at each side: a circle's radius, or a box as `BodyBox::grown` grows it;
    /// `None` past `MAX_BOUND`.
    pub fn grown(&self, by: Num) -> Option<Self> {
        match self {
            Self::Circle(radius) => Radius::new(radius.get().checked_add(by)?).map(Self::Circle),
            Self::Box(body) => body.grown(by).map(Self::Box),
        }
    }

    /// How far it reaches from the position along x and along z.
    pub fn extent(&self) -> [Num; 2] {
        match self {
            Self::Circle(radius) => [radius.get(); 2],
            Self::Box(body) => body.extent(),
        }
    }

    /// The point of the shape at `position` nearest `to` on the ground plane, at `position`'s
    /// height: `to` itself inside; a box's nearest point, each coordinate rounded to the nearest
    /// bit; a circle's, a step of its radius from its center toward `to`, as a walker steps.
    pub fn nearest_point(&self, position: Vec3, to: Vec3) -> Vec3 {
        let ground = Vec3::new(to.x, position.y, to.z);
        match self {
            Self::Box(body) => body.nearest_point(position, ground),
            Self::Circle(radius) if position.within_ground(ground, radius.get()) => ground,
            Self::Circle(radius) => position.step_toward(ground, radius.get()),
        }
    }

    /// `point` moved out of the shape at `position` when a body of `radius` there overlaps it: a
    /// box's as `BodyBox::push_out` moves it; a circle's straight away from its center, along x
    /// from the center itself, until the two bodies are at least the sum of their radii apart,
    /// so they touch and do not overlap. `None` when they do not overlap: touching is not
    /// overlap.
    pub fn push_out(&self, position: Vec3, point: Vec3, radius: Num) -> Option<Vec3> {
        let own = match self {
            Self::Box(body) => return body.push_out(position, point, radius),
            Self::Circle(own) => own.get(),
        };
        let off =
            [point.x - position.x, point.z - position.z].map(|axis| i128::from(axis.to_bits()));
        let reach = i128::from(own.to_bits()) + i128::from(radius.to_bits());
        if off[0] * off[0] + off[1] * off[1] >= reach * reach {
            return None;
        }
        let away = Direction::of(off).map_or([reach, 0], |direction| direction.scaled_away(reach));
        let axis = |center: Num, by: i128| {
            Num::from_bits(
                i64::try_from(i128::from(center.to_bits()) + by).expect("a push within the world"),
            )
        };
        Some(Vec3::new(
            axis(position.x, away[0]),
            point.y,
            axis(position.z, away[1]),
        ))
    }
}
