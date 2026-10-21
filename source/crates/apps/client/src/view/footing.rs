use bevy::math::{Quat, Vec2, Vec3, Vec3Swizzles};

/// What a unit's drawing covers of the ground round its center: a circle of a radius, or a box
/// of half sides `half` along its turned axes, turned `yaw` radians about the vertical, as Bevy
/// turns a transform.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Footing {
    Circle(f32),
    Box { half: [f32; 2], yaw: f32 },
}

/// The stretch of a line, from `near` to `far` along it, over which it stands within a shape;
/// either end infinite for a line that never leaves it that way.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Span {
    pub(crate) near: f32,
    pub(crate) far: f32,
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

    /// Where the line through `from`, a point measured from its center, along `way` stands over
    /// it, its radius and each half side widened to at least `least`, in multiples of `way`:
    /// the whole line for one that stands over it everywhere, none for one that never does.
    pub(crate) fn span(self, from: Vec3, way: Vec3, least: f32) -> Option<Span> {
        match self {
            Self::Circle(radius) => circle_span(radius.max(least), from.xz(), way.xz()),
            Self::Box { half, yaw } => {
                let back = Quat::from_rotation_y(-yaw);
                let (from, way) = (back * from, back * way);
                let x = Span::slab(half[0].max(least), from.x, way.x)?;
                let z = Span::slab(half[1].max(least), from.z, way.z)?;
                x.and(z)
            }
        }
    }
}

impl Span {
    const WHOLE: Self = Self {
        near: f32::NEG_INFINITY,
        far: f32::INFINITY,
    };

    /// Where the line through `from` along `way` stands within `half` of 0 on one axis.
    pub(crate) fn slab(half: f32, from: f32, way: f32) -> Option<Self> {
        if way == 0.0 {
            return (from.abs() <= half).then_some(Self::WHOLE);
        }
        let [a, b] = [(-half - from) / way, (half - from) / way];
        Some(Self {
            near: a.min(b),
            far: a.max(b),
        })
    }

    /// The stretch within both; none when they do not meet.
    pub(crate) fn and(self, other: Self) -> Option<Self> {
        let near = self.near.max(other.near);
        let far = self.far.min(other.far);
        (near <= far).then_some(Self { near, far })
    }
}

/// Where the line through `from` along `way`, on a plane, stands within `radius` of its origin:
/// the roots of |from + t · way|² = radius², the smaller one taken as c / q so that neither loses
/// its digits to a difference of near equals.
fn circle_span(radius: f32, from: Vec2, way: Vec2) -> Option<Span> {
    let a = way.length_squared();
    let c = from.length_squared() - radius * radius;
    if a == 0.0 {
        return (c <= 0.0).then_some(Span::WHOLE);
    }
    let half_b = from.dot(way);
    let discriminant = half_b * half_b - a * c;
    if discriminant < 0.0 {
        return None;
    }
    let q = -(half_b + half_b.signum() * discriminant.sqrt());
    if q == 0.0 {
        return Some(Span {
            near: 0.0,
            far: 0.0,
        });
    }
    let [one, other] = [q / a, c / q];
    Some(Span {
        near: one.min(other),
        far: one.max(other),
    })
}
