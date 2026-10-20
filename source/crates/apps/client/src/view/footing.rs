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
    /// The radius of the least circle round its center that holds it.
    pub(crate) fn bound(self) -> f32 {
        match self {
            Self::Circle(radius) => radius,
            Self::Box { half, .. } => Vec2::from(half).length(),
        }
    }

    /// The turn it stands at: a box's yaw, none for a circle.
    pub(crate) fn upright(self) -> Quat {
        match self {
            Self::Circle(_) => Quat::IDENTITY,
            Self::Box { yaw, .. } => Quat::from_rotation_y(yaw),
        }
    }
}
