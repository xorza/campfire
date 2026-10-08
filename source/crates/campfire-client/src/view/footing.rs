use bevy::math::{Quat, Vec2};

/// What a unit's drawing covers of the ground round its center: a circle of a radius, or a box
/// of half sides `half` along its turned axes, turned `yaw` radians about the vertical, as Bevy
/// turns a transform.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Footing {
    Circle(f32),
    Box { half: [f32; 2], yaw: f32 },
}

impl Footing {
    /// Whether it covers the point `offset` from its center, edge included.
    pub(crate) fn covers(self, offset: Vec2) -> bool {
        match self {
            Footing::Circle(radius) => offset.length_squared() <= radius * radius,
            Footing::Box { half, yaw } => {
                // The point in the box's own axes: Bevy turns x by `yaw` to (cos, −sin).
                let along = Vec2::new(yaw.cos(), -yaw.sin());
                let across = Vec2::new(yaw.sin(), yaw.cos());
                offset.dot(along).abs() <= half[0] && offset.dot(across).abs() <= half[1]
            }
        }
    }

    /// The radius of the least circle round its center that holds it.
    pub(crate) fn bound(self) -> f32 {
        match self {
            Footing::Circle(radius) => radius,
            Footing::Box { half, .. } => Vec2::from(half).length(),
        }
    }

    /// The turn it stands at: a box's yaw, none for a circle.
    pub(crate) fn upright(self) -> Quat {
        match self {
            Footing::Circle(_) => Quat::IDENTITY,
            Footing::Box { yaw, .. } => Quat::from_rotation_y(yaw),
        }
    }
}
