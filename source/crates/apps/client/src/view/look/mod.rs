use std::f32::consts::FRAC_PI_2;

use bevy::asset::Handle;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::math::{Quat, Vec2, Vec3};
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::picking::Pickable;
use bevy::shape::Ray3d;
use bevy::transform::components::Transform;
use campfire_units::Body;

use crate::view::float_num::FloatNum;
use crate::view::footing::Footing;

/// On the drawing root of a unit drawn as its shape: the figure that draws it, the root's child,
/// and the material it wears while the unit lives. A dead unit lies on the ground, gray.
#[derive(Component, Debug)]
pub(crate) struct Look {
    pub(crate) figure: Entity,
    pub(crate) alive: Handle<StandardMaterial>,
}

/// On every unit's drawing root, the shape of the unit, which the HUD stands over and a dead unit
/// lies down by, whether its figure or its models draw it: a player's avatar, a creep that walks, or
/// a structure that does not, each of its kind's proportions, and a model drawing as tall as its
/// models. A capsule of `radius`, `height` tall, or, for a box body, a cuboid as tall standing on
/// its `footing`. The pointer picks the unit by the root, which blocks the units it stands in front
/// of.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
#[require(Pickable)]
pub(crate) struct UnitShape {
    pub(crate) radius: f32,
    pub(crate) height: f32,
    pub(crate) footing: Footing,
}

/// What a unit is drawn as when no model draws it: a player's avatar, a creep that walks, or a
/// structure that does not, each with its own shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShapeKind {
    Avatar,
    Creep,
    Structure,
}

/// Whether a figure's unit lives or lies dead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Condition {
    Alive,
    Dead,
}

/// What a figure wears and how it stands over its root.
#[derive(Debug, Clone)]
pub(crate) struct Pose {
    pub(crate) material: MeshMaterial3d<StandardMaterial>,
    pub(crate) transform: Transform,
}

const AVATAR: UnitShape = UnitShape {
    radius: 0.5,
    height: 2.0,
    footing: Footing::Circle(0.5),
};
const CREEP: UnitShape = UnitShape {
    radius: 0.35,
    height: 1.2,
    footing: Footing::Circle(0.35),
};
const STRUCTURE: UnitShape = UnitShape {
    radius: 0.9,
    height: 3.8,
    footing: Footing::Circle(0.9),
};

impl ShapeKind {
    /// The kind of a unit that is a player's avatar if `avatar`, and walks if `walks`.
    pub(crate) const fn of(avatar: bool, walks: bool) -> Self {
        match (avatar, walks) {
            (true, _) => Self::Avatar,
            (false, true) => Self::Creep,
            (false, false) => Self::Structure,
        }
    }

    /// Its shape with no body.
    const fn shape(self) -> UnitShape {
        match self {
            Self::Avatar => AVATAR,
            Self::Creep => CREEP,
            Self::Structure => STRUCTURE,
        }
    }
}

impl Condition {
    /// The condition of a unit that is dead if `dead`.
    pub(crate) const fn of(dead: bool) -> Self {
        if dead { Self::Dead } else { Self::Alive }
    }
}

impl Look {
    /// The figure's material and its pose over the root for a unit of `shape` in `condition`,
    /// `dead_material` the one a dead unit wears. A capsule lies down when its unit dies; a box
    /// stands where it stood, as a ruin.
    pub(crate) fn pose(
        &self,
        shape: UnitShape,
        condition: Condition,
        dead_material: &Handle<StandardMaterial>,
    ) -> Pose {
        let UnitShape {
            radius,
            height,
            footing,
        } = shape;
        let dead = condition == Condition::Dead;
        let material = if dead { dead_material } else { &self.alive };
        let transform = if dead && matches!(footing, Footing::Circle(_)) {
            Transform::from_translation(Vec3::Y * radius)
                .with_rotation(Quat::from_rotation_z(FRAC_PI_2))
        } else {
            Transform::from_translation(Vec3::Y * (height / 2.0)).with_rotation(footing.upright())
        };
        Pose {
            material: MeshMaterial3d(material.clone()),
            transform,
        }
    }
}

impl UnitShape {
    /// The shape of a unit of `kind` that has `body`, drawn by models whose top is `top`: as wide
    /// as its body's circle, or as its box's shorter half edge, its kind's width with no body; as
    /// tall as its body when the body has a height, as Zero Hour stands a bar over its geometry's
    /// height, else as its models, else as its kind for its width, whatever the scale of its game.
    pub(crate) fn of(kind: ShapeKind, body: Option<&Body>, top: Option<f32>) -> Self {
        let kind = kind.shape();
        let footing = match (
            body.and_then(|body| body.radius()),
            body.and_then(|body| body.half_edges()),
        ) {
            (Some(radius), _) => Footing::Circle(radius.float()),
            (None, Some([a, b])) => {
                let [a, b] = [a, b].map(|edge| Vec2::new(edge[0].float(), edge[1].float()));
                Footing::Box {
                    half: [a.length(), b.length()],
                    yaw: (-a.y).atan2(a.x),
                }
            }
            (None, None) => Footing::Circle(kind.radius),
        };
        let radius = match footing {
            Footing::Circle(radius) => radius,
            Footing::Box { half, .. } => half[0].min(half[1]),
        };
        let raised = body
            .map(|body| body.height().float())
            .filter(|&height| height > 0.0);
        let height = (raised.or_else(|| top.map(|top| top.max(0.0))))
            .unwrap_or(kind.height / kind.radius * radius);
        Self {
            radius,
            height,
            footing,
        }
    }

    /// The length of its capsule between the caps.
    pub(crate) fn length(&self) -> f32 {
        (self.height - 2.0 * self.radius).max(0.0)
    }

    /// How far it reaches from its axis: the radius of the least circle that holds what it covers
    /// of the ground.
    pub(crate) fn reach(&self) -> f32 {
        self.footing.bound()
    }

    /// How far along `ray` it first meets the upright cylinder of `radius` round its axis, as
    /// tall as it, standing on `base`, in meters: the least size the pointer keeps a unit at
    /// (02-engine-core.md, A least size on screen); 0 from inside, none when the ray misses it or
    /// meets it only behind its origin.
    pub(crate) fn axis_entry(&self, base: Vec3, ray: Ray3d, radius: f32) -> Option<f32> {
        let (from, way) = (ray.origin - base, *ray.direction);
        let half = self.height / 2.0;
        let ground = Span::circle(radius, Vec2::new(from.x, from.z), Vec2::new(way.x, way.z))?;
        let rise = Span::slab(half, from.y - half, way.y)?;
        Some(ground.and(rise)?.and(Span::AHEAD)?.near)
    }
}

/// The stretch of a line, from `near` to `far` along it in multiples of its way, over which it
/// stands within a shape; either end infinite for a line that never leaves it that way.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Span {
    near: f32,
    far: f32,
}

impl Span {
    const WHOLE: Self = Self {
        near: f32::NEG_INFINITY,
        far: f32::INFINITY,
    };
    const AHEAD: Self = Self {
        near: 0.0,
        far: f32::INFINITY,
    };

    /// Where the line through `from` along `way` stands within `half` of 0 on one axis.
    fn slab(half: f32, from: f32, way: f32) -> Option<Self> {
        if way == 0.0 {
            return (from.abs() <= half).then_some(Self::WHOLE);
        }
        let [a, b] = [(-half - from) / way, (half - from) / way];
        Some(Self {
            near: a.min(b),
            far: a.max(b),
        })
    }

    /// Where the line through `from` along `way`, on a plane, stands within `radius` of its
    /// origin: the roots of |from + t · way|² = radius², the smaller one taken as c / q so that
    /// neither loses its digits to a difference of near equals.
    fn circle(radius: f32, from: Vec2, way: Vec2) -> Option<Self> {
        let a = way.length_squared();
        let c = from.length_squared() - radius * radius;
        if a == 0.0 {
            return (c <= 0.0).then_some(Self::WHOLE);
        }
        let half_b = from.dot(way);
        let discriminant = half_b * half_b - a * c;
        if discriminant < 0.0 {
            return None;
        }
        let q = -(half_b + half_b.signum() * discriminant.sqrt());
        if q == 0.0 {
            return Some(Self {
                near: 0.0,
                far: 0.0,
            });
        }
        let [one, other] = [q / a, c / q];
        Some(Self {
            near: one.min(other),
            far: one.max(other),
        })
    }

    /// The stretch within both; none when they do not meet.
    fn and(self, other: Self) -> Option<Self> {
        let near = self.near.max(other.near);
        let far = self.far.min(other.far);
        (near <= far).then_some(Self { near, far })
    }
}

#[cfg(test)]
mod tests;
