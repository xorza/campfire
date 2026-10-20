use campfire_math::{Num, Vec3};
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
}
