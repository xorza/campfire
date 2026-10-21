use std::f32::consts::SQRT_2;

use bevy::ecs::world::World;
use bevy::math::Dir3;
use campfire_math::Num;
use campfire_units::BodyForm;

use super::*;

#[test]
fn a_unit_is_drawn_at_its_bodys_radius_and_height_else_its_models_or_kinds_and_lies_down_dead() {
    let body = Body::new(Num::from_bits(3 << (Num::FRAC_BITS - 2))).unwrap();
    // A radius of 0.75 m, of an avatar's proportions, 2 m tall at 0.5 m: 3 m tall.
    let avatar = UnitShape {
        radius: 0.75,
        height: 3.0,
        footing: Footing::Circle(0.75),
    };
    assert_eq!(UnitShape::of(ShapeKind::Avatar, Some(&body), None), avatar);
    assert_eq!(UnitShape::of(ShapeKind::Creep, None, None), CREEP);
    assert_eq!(UnitShape::of(ShapeKind::Structure, None, None), STRUCTURE);
    // A box of 4 × 2 m turned a quarter: its first half edge points along −z, which Bevy's
    // turn of π/2 about y takes x to, as the unit's facing of 90° turns its model; it stands
    // as tall as a structure 1 m wide, its shorter half edge, and reaches √5 m.
    let form = BodyForm::box_sized([Num::int(4), Num::int(2)]).unwrap();
    let turned = form.at(Num::int(90));
    let boxed = UnitShape::of(ShapeKind::Structure, Some(&turned), None);
    let expected = Footing::Box {
        half: [2.0, 1.0],
        yaw: FRAC_PI_2,
    };
    assert_eq!(boxed.footing, expected);
    assert_eq!(
        (boxed.radius, boxed.height),
        (1.0, STRUCTURE.height / STRUCTURE.radius)
    );
    // Models whose top is 89 m, as a command center's radar tower over its 52 m building: a body
    // 49 m tall stands 49 m tall, as its geometry does; one of no height as tall as its models,
    // and a top below the ground at the ground; with neither, its kind's height.
    let raised = turned.raised(Num::int(49)).unwrap();
    let height = |body: Option<&Body>, top| UnitShape::of(ShapeKind::Structure, body, top).height;
    assert_eq!(
        [
            height(Some(&raised), Some(89.0)),
            height(Some(&raised), None),
            height(Some(&turned), Some(89.0)),
            height(None, Some(-0.5)),
            height(None, None),
        ],
        [49.0, 49.0, 89.0, 0.0, STRUCTURE.height]
    );
    let figure = World::new().spawn_empty().id();
    let look = Look {
        figure,
        alive: Handle::default(),
    };
    assert!((boxed.reach() - 5.0_f32.sqrt()).abs() < 1e-6);
    assert!(
        Quat::from_rotation_y(-FRAC_PI_2)
            .mul_vec3(Vec3::X)
            .abs_diff_eq(Vec3::Z, 1e-6)
    );
    // The avatar's capsule stands its half height up, 1.5 m; dead, it lies on its side, its
    // axis its radius over the ground. The box stands as it stood.
    let dead = Handle::default();
    let standing = look.pose(avatar, Condition::Alive, &dead).transform;
    let lying = look.pose(avatar, Condition::Dead, &dead).transform;
    assert_eq!(standing.translation, Vec3::Y * 1.5);
    assert_eq!(
        (lying.translation, lying.rotation),
        (Vec3::Y * 0.75, Quat::from_rotation_z(FRAC_PI_2))
    );
    assert_eq!(
        look.pose(boxed, Condition::Dead, &dead).transform,
        look.pose(boxed, Condition::Alive, &dead).transform
    );
}

#[test]
fn a_ray_enters_the_cylinder_round_a_shape_s_axis_at_its_side_or_its_top() {
    let ray = |origin: Vec3, way: Vec3| Ray3d::new(origin, Dir3::new(way).unwrap());
    // A shape 2 m tall standing on the origin, and the cylinder of radius 0.5 round its axis. A
    // level ray at 1 m from z = 10 along −z meets its side at z = 0.5, 9.5 along; at 2.5 m it
    // passes over its top. A ray from (0, 3, 1) down at 45° along −z passes over its edge at
    // z = 0.5, 2.5 m high, and enters its top at (0, 2, 0), √2 along. From inside it meets it
    // where it starts; pointed away, never.
    let post = UnitShape {
        radius: 0.5,
        height: 2.0,
        footing: Footing::Circle(0.5),
    };
    let at = Vec3::ZERO;
    let level = ray(Vec3::new(0.0, 1.0, 10.0), Vec3::NEG_Z);
    assert_eq!(post.axis_entry(at, level, 0.5), Some(9.5));
    let over = ray(Vec3::new(0.0, 2.5, 10.0), Vec3::NEG_Z);
    assert_eq!(post.axis_entry(at, over, 0.5), None);
    let down = ray(Vec3::new(0.0, 3.0, 1.0), Vec3::new(0.0, -1.0, -1.0));
    let top = post.axis_entry(at, down, 0.5).unwrap();
    assert!((top - SQRT_2).abs() < 1e-6, "{top}");
    assert_eq!(
        post.axis_entry(at, ray(Vec3::Y, Vec3::NEG_Z), 0.5),
        Some(0.0)
    );
    let away = ray(Vec3::new(0.0, 1.0, 10.0), Vec3::Z);
    assert_eq!(post.axis_entry(at, away, 0.5), None);
    // A level ray 1 m to its side misses a radius of 0.5, and meets one of 1.25 where
    // x² + z² = 1.25², at z = 0.75, 9.25 along; standing at (0, 0, −3), 3 m farther.
    let beside = ray(Vec3::new(1.0, 1.0, 10.0), Vec3::NEG_Z);
    assert_eq!(post.axis_entry(at, beside, 0.5), None);
    assert_eq!(post.axis_entry(at, beside, 1.25), Some(9.25));
    assert_eq!(
        post.axis_entry(Vec3::NEG_Z * 3.0, beside, 1.25),
        Some(12.25)
    );
}
