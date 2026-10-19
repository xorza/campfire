use std::ops::Range;

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
    /// Where the line from `from`, an offset from its center, along `way` runs inside it: the
    /// span of the line's parameter, edges included, from `-inf` to `inf` along a line that never
    /// leaves it; none when the line misses it.
    pub(crate) fn crossed(self, from: Vec2, way: Vec2) -> Option<Range<f32>> {
        match self {
            Self::Circle(radius) => {
                let a = way.length_squared();
                let c = from.length_squared() - radius * radius;
                if a == 0.0 {
                    return (c <= 0.0).then_some(f32::NEG_INFINITY..f32::INFINITY);
                }
                let b = from.dot(way);
                let quarter = b * b - a * c;
                if quarter < 0.0 {
                    return None;
                }
                let root = quarter.sqrt();
                Some((-b - root) / a..(-b + root) / a)
            }
            Self::Box { half, yaw } => {
                let along = Vec2::new(yaw.cos(), -yaw.sin());
                let across = Vec2::new(yaw.sin(), yaw.cos());
                let first = span(from.dot(along), way.dot(along), half[0])?;
                let second = span(from.dot(across), way.dot(across), half[1])?;
                let near = first.start.max(second.start);
                let far = first.end.min(second.end);
                (near <= far).then_some(near..far)
            }
        }
    }

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

/// Where the line from `from` along `way`, on one axis, runs within `half` of 0: the span of its
/// parameter, edges included; none when it never does.
pub(crate) fn span(from: f32, way: f32, half: f32) -> Option<Range<f32>> {
    if way == 0.0 {
        return (from.abs() <= half).then_some(f32::NEG_INFINITY..f32::INFINITY);
    }
    let (enter, leave) = ((-half - from) / way, (half - from) / way);
    Some(enter.min(leave)..enter.max(leave))
}
