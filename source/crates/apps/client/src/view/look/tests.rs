use std::f32::consts::SQRT_2;

use bevy::ecs::world::World;
use bevy::math::Dir3;
use campfire_math::Num;
use campfire_units::BodyForm;

use super::*;

#[test]
fn a_unit_is_drawn_at_its_bodys_radius_and_its_kinds_height_and_lies_down_dead() {
    let body = Body::new(Num::from_bits(3 << (Num::FRAC_BITS - 2))).unwrap();
    // A radius of 0.75 m, of an avatar's proportions, 2 m tall at 0.5 m: 3 m tall.
    let avatar = UnitShape {
        radius: 0.75,
        height: 3.0,
        footing: Footing::Circle(0.75),
    };
    assert_eq!(UnitShape::of(ShapeKind::Avatar, Some(&body)), avatar);
    assert_eq!(UnitShape::of(ShapeKind::Creep, None), CREEP);
    assert_eq!(UnitShape::of(ShapeKind::Structure, None), STRUCTURE);
    // A box of 4 × 2 m turned a quarter: its first half edge points along −z, which Bevy's
    // turn of π/2 about y takes x to, as the unit's facing of 90° turns its model; it stands
    // as tall as a structure 1 m wide, its shorter half edge, and reaches √5 m.
    let form = BodyForm::box_sized([Num::int(4), Num::int(2)]).unwrap();
    let boxed = UnitShape::of(ShapeKind::Structure, Some(&form.at(Num::int(90))));
    let expected = Footing::Box {
        half: [2.0, 1.0],
        yaw: FRAC_PI_2,
    };
    assert_eq!(boxed.footing, expected);
    assert_eq!(
        (boxed.radius, boxed.height),
        (1.0, STRUCTURE.height / STRUCTURE.radius)
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
fn a_ray_enters_a_shape_at_its_side_or_its_top_and_a_least_radius_widens_it() {
    let ray = |origin: Vec3, way: Vec3| Ray3d::new(origin, Dir3::new(way).unwrap());
    // An upright cylinder of radius 0.5, 2 m tall, standing on the origin. A level ray at 1 m
    // from z = 10 along −z meets its side at z = 0.5, 9.5 along; at 2.5 m it passes over its
    // top. A ray from (0, 3, 1) down at 45° along −z passes over its edge at z = 0.5, 2.5 m
    // high, and enters its top at (0, 2, 0), √2 along. From inside it meets it where it starts;
    // pointed away, never.
    let post = UnitShape {
        radius: 0.5,
        height: 2.0,
        footing: Footing::Circle(0.5),
    };
    let at = Vec3::ZERO;
    assert_eq!(
        post.entry(at, ray(Vec3::new(0.0, 1.0, 10.0), Vec3::NEG_Z), 0.0),
        Some(9.5)
    );
    assert_eq!(
        post.entry(at, ray(Vec3::new(0.0, 2.5, 10.0), Vec3::NEG_Z), 0.0),
        None
    );
    let down = ray(Vec3::new(0.0, 3.0, 1.0), Vec3::new(0.0, -1.0, -1.0));
    let top = post.entry(at, down, 0.0).unwrap();
    assert!((top - SQRT_2).abs() < 1e-6, "{top}");
    assert_eq!(post.entry(at, ray(Vec3::Y, Vec3::NEG_Z), 0.0), Some(0.0));
    assert_eq!(
        post.entry(at, ray(Vec3::new(0.0, 1.0, 10.0), Vec3::Z), 0.0),
        None
    );
    // A level ray 1 m to its side misses it; widened to a radius of 1.25 m, it meets its side
    // where x² + z² = 1.25², at z = 0.75, 9.25 along. Standing at (0, 0, −3), the shape is met
    // 3 m farther.
    let beside = ray(Vec3::new(1.0, 1.0, 10.0), Vec3::NEG_Z);
    assert_eq!(post.entry(at, beside, 0.0), None);
    assert_eq!(post.entry(at, beside, 1.25), Some(9.25));
    assert_eq!(post.entry(Vec3::NEG_Z * 3.0, beside, 1.25), Some(12.25));

    // A box of half sides 2 and 1, 2 m tall, turned a quarter: its first half side lies along
    // z, so it spans x −1 to 1 and z −2 to 2. A level ray along x at z = 1.9 meets its face at
    // x = −1, 9 along from x = −10; at z = 2.1 it misses, and so does the unturned box, which
    // spans z −1 to 1, at z = 1.9. The quarter turn's cosine rounds to −4.4e-8 in f32, not 0,
    // which moves the entry far below 1e-5.
    let turned = Footing::Box {
        half: [2.0, 1.0],
        yaw: FRAC_PI_2,
    };
    let hull = UnitShape {
        radius: 1.0,
        height: 2.0,
        footing: turned,
    };
    let along = |z| ray(Vec3::new(-10.0, 1.0, z), Vec3::X);
    let face = hull.entry(at, along(1.9), 0.0).unwrap();
    assert!((face - 9.0).abs() < 1e-5, "{face}");
    assert_eq!(hull.entry(at, along(2.1), 0.0), None);
    let unturned = UnitShape {
        footing: Footing::Box {
            half: [2.0, 1.0],
            yaw: 0.0,
        },
        ..hull
    };
    assert_eq!(unturned.entry(at, along(1.9), 0.0), None);
    // Its corner (1, 2) lies on the line x + z = 3. A level ray along (1, 0, −1) on x + z = 2.9
    // enters its face z = 2 at x = 0.9, 8 m along z from z = 10, 8√2 along; on x + z = 3.1 it
    // leaves the slab of x at z = 2.1, before it reaches z = 2, and passes the corner.
    let diagonal = |sum: f32| ray(Vec3::new(sum - 10.0, 1.0, 10.0), Vec3::new(1.0, 0.0, -1.0));
    let corner = hull.entry(at, diagonal(2.9), 0.0).unwrap();
    assert!((corner - 8.0 * SQRT_2).abs() < 1e-5, "{corner}");
    assert_eq!(hull.entry(at, diagonal(3.1), 0.0), None);
}
