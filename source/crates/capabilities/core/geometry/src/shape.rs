use campfire_math::{Num, Vec3};
use campfire_sim::Position;
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
    pub const POINT: Shape = Shape::Circle(Radius::ZERO);

    /// The radius of the least circle round the position that holds it.
    pub const fn bound(&self) -> Num {
        match self {
            Shape::Circle(radius) => radius.get(),
            Shape::Box(body) => body.bound(),
        }
    }

    /// How far it reaches from the position along x and along z.
    pub fn extent(&self) -> [Num; 2] {
        match self {
            Shape::Circle(radius) => [radius.get(); 2],
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
            Shape::Circle(radius) if pos.within_ground(ground, radius.get()) => ground,
            Shape::Circle(radius) => {
                let step = pos.get().step_toward(ground.get(), radius.get());
                Position::new(step).expect("a step within a body's radius stays within the bound")
            }
        }
    }
}
