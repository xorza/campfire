use std::f32::consts::FRAC_PI_2;

use bevy::asset::Handle;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::math::{Quat, Vec2, Vec3};
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::transform::components::Transform;
use campfire_capabilities::Body;

use crate::view::View;
use crate::view::footing::Footing;

/// On a unit's drawing root: the figure that draws the unit, the root's child, the material it
/// wears while the unit lives, and its shape. A dead unit lies on the ground, gray.
#[derive(Component, Debug)]
pub(crate) struct Look {
    pub(crate) figure: Entity,
    pub(crate) alive: Handle<StandardMaterial>,
    pub(crate) shape: Shape,
}

/// The shape of a unit: an avatar is under a player's control, a structure does not walk. A
/// capsule of `radius` and `length`, or, for a box body, a cuboid as tall standing on its
/// `footing`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Shape {
    pub(crate) radius: f32,
    pub(crate) length: f32,
    pub(crate) footing: Footing,
}

/// What a figure wears and how it stands over its root.
#[derive(Debug, Clone)]
pub(crate) struct Pose {
    pub(crate) material: MeshMaterial3d<StandardMaterial>,
    pub(crate) transform: Transform,
}

const AVATAR: Shape = Shape {
    radius: 0.5,
    length: 1.0,
    footing: Footing::Circle(0.5),
};
const CREEP: Shape = Shape {
    radius: 0.35,
    length: 0.5,
    footing: Footing::Circle(0.35),
};
const STRUCTURE: Shape = Shape {
    radius: 0.9,
    length: 2.0,
    footing: Footing::Circle(0.9),
};

impl Look {
    /// How far the drawing reaches from its axis: the radius of the least circle that holds what
    /// it covers of the ground.
    pub(crate) fn radius(&self) -> f32 {
        self.shape.footing.bound()
    }

    /// What the drawing covers of the ground.
    pub(crate) const fn footing(&self) -> Footing {
        self.shape.footing
    }

    /// How tall the drawing stands while its unit lives.
    pub(crate) fn height(&self) -> f32 {
        self.shape.height()
    }

    /// The figure's material and its pose over the root for a unit that is `dead` or alive,
    /// `dead_material` the one a dead unit wears. A capsule lies down when its unit dies; a box
    /// stands where it stood, as a ruin.
    pub(crate) fn pose(&self, dead: bool, dead_material: &Handle<StandardMaterial>) -> Pose {
        let Shape {
            radius,
            length,
            footing,
        } = self.shape;
        let material = if dead { dead_material } else { &self.alive };
        let transform = if dead && matches!(footing, Footing::Circle(_)) {
            Transform::from_translation(Vec3::Y * radius)
                .with_rotation(Quat::from_rotation_z(FRAC_PI_2))
        } else {
            Transform::from_translation(Vec3::Y * (radius + length / 2.0))
                .with_rotation(footing.upright())
        };
        Pose {
            material: MeshMaterial3d(material.clone()),
            transform,
        }
    }
}

impl Shape {
    /// The shape of a unit that is under a player's control if `owned`, walks if `walks`, and has
    /// `body`: its kind's height, at its body's radius, or on its box, or its kind's own with no
    /// body.
    pub(crate) fn of(owned: bool, walks: bool, body: Option<&Body>) -> Shape {
        let kind = match (owned, walks) {
            (true, _) => AVATAR,
            (false, true) => CREEP,
            (false, false) => STRUCTURE,
        };
        let footing = match (
            body.and_then(|body| body.radius()),
            body.and_then(|body| body.half_edges()),
        ) {
            (Some(radius), _) => Footing::Circle(View::float(radius)),
            (None, Some([a, b])) => {
                let [a, b] =
                    [a, b].map(|edge| Vec2::new(View::float(edge[0]), View::float(edge[1])));
                Footing::Box {
                    half: [a.length(), b.length()],
                    yaw: (-a.y).atan2(a.x),
                }
            }
            (None, None) => Footing::Circle(kind.radius),
        };
        let radius = match footing {
            Footing::Circle(radius) => radius,
            Footing::Box { .. } => kind.radius,
        };
        Shape {
            radius,
            footing,
            ..kind
        }
    }

    /// How tall it stands.
    pub(crate) fn height(&self) -> f32 {
        self.length + 2.0 * self.radius
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::world::World;
    use campfire_capabilities::BodyForm;
    use campfire_math::Num;

    use super::*;

    #[test]
    fn a_unit_is_drawn_at_its_bodys_radius_and_its_kinds_height_and_lies_down_dead() {
        let body = Body::new(Num::from_bits(3 << (Num::FRAC_BITS - 2))).unwrap();
        let avatar = Shape {
            radius: 0.75,
            footing: Footing::Circle(0.75),
            ..AVATAR
        };
        assert_eq!(Shape::of(true, true, Some(&body)), avatar);
        assert_eq!(Shape::of(false, true, None), CREEP);
        assert_eq!(Shape::of(false, false, None), STRUCTURE);
        // A box of 4 × 2 m turned a quarter: its first half edge points along +z, which Bevy's
        // turn of -π/2 about y takes x to; it stands as tall as a structure, and reaches √5 m.
        let form = BodyForm::boxed([Num::int(4), Num::int(2)]).unwrap();
        let boxed = Shape::of(false, false, Some(&form.at(Num::int(90))));
        let expected = Footing::Box {
            half: [2.0, 1.0],
            yaw: -FRAC_PI_2,
        };
        assert_eq!(boxed.footing, expected);
        assert_eq!(boxed.height(), STRUCTURE.height());
        let figure = World::new().spawn_empty().id();
        let look = |shape| Look {
            figure,
            alive: Handle::default(),
            shape,
        };
        assert!((look(boxed).radius() - 5.0_f32.sqrt()).abs() < 1e-6);
        assert!(
            Quat::from_rotation_y(-FRAC_PI_2)
                .mul_vec3(Vec3::X)
                .abs_diff_eq(Vec3::Z, 1e-6)
        );
        // It covers a point 1.9 m along z and 0.9 m along x from its center, and not 1.1 m along x.
        assert!(expected.covers(Vec2::new(0.9, 1.9)));
        assert!(!expected.covers(Vec2::new(1.1, 0.0)));
        // The avatar's capsule stands its half length over its radius, 0.75 + 0.5; dead, it lies
        // on its side, its axis its radius over the ground. The box stands as it stood.
        let dead = Handle::default();
        let standing = look(avatar).pose(false, &dead).transform;
        let lying = look(avatar).pose(true, &dead).transform;
        assert_eq!(standing.translation, Vec3::Y * 1.25);
        assert_eq!(
            (lying.translation, lying.rotation),
            (Vec3::Y * 0.75, Quat::from_rotation_z(FRAC_PI_2))
        );
        assert_eq!(
            look(boxed).pose(true, &dead).transform,
            look(boxed).pose(false, &dead).transform
        );
    }
}
