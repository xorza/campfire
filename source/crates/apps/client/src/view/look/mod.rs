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
use crate::view::footing::{Footing, Span};

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
/// its `footing`. The pointer picks the unit by it, so the root blocks the units it stands in
/// front of.
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
    /// The shape of a unit of `kind` that has `body`, of its kind's proportions at its body's
    /// size, whatever the scale of its game: as wide as its body's circle, or as its box's
    /// shorter half edge, and as tall as its kind for that width; its kind's own with no body.
    pub(crate) fn of(kind: ShapeKind, body: Option<&Body>) -> Self {
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
        Self {
            radius,
            height: kind.height / kind.radius * radius,
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

    /// How far along `ray` it first meets the shape standing on `base`, in meters: its footing,
    /// turned as the world turns it whatever turns its root, raised from `base` to its height, an
    /// upright cylinder or a turned box, its radius and each half side widened to at least
    /// `least` meters; 0 from inside, none when the ray misses it or meets it only behind its
    /// origin.
    pub(crate) fn entry(&self, base: Vec3, ray: Ray3d, least: f32) -> Option<f32> {
        let (from, way) = (ray.origin - base, *ray.direction);
        let half = self.height / 2.0;
        let ground = self.footing.span(from, way, least)?;
        let rise = Span::slab(half, from.y - half, way.y)?;
        let ahead = Span {
            near: 0.0,
            far: f32::INFINITY,
        };
        Some(ground.and(rise)?.and(ahead)?.near)
    }
}

#[cfg(test)]
mod tests;
