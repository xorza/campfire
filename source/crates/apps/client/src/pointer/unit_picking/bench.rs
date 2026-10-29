use std::hint::black_box;

use bevy::asset::Assets;
use bevy::camera::Projection;
use bevy::ecs::world::World;
use bevy::math::{Dir3, Quat, Vec3};
use bevy::mesh::{Mesh, MeshBuilder, Meshable};
use bevy::shape::{Cuboid, Ray3d, Sphere};
use bevy::transform::components::{GlobalTransform, Transform};
use campfire_measure::{Kind, Scale};
use campfire_sim::IdAllocator;
use campfire_values::Team;
use criterion::Criterion;

use crate::pointer::Pointed;
use crate::pointer::unit_picking::UnitPicking;
use crate::pointer::unit_picking::pick_scene::{Part, PartAssets, PartSetup, PickScene, Target};
use crate::view::file_material::FileMaterial;
use crate::view::footing::Footing;
use crate::view::look::UnitShape;
use crate::view::pixel_scale::PixelScale;
use crate::view::view_dice::ViewDice;

/// The units of the scene, as many as a large Zero Hour battle holds.
const COUNT: usize = 1000;

/// `COUNT` units on a square of 128 m, each drawn by one mesh, its least radius 10 pixels as
/// `scale` shows it: of every eight, seven spheres of 320 triangles and radius 0.3 to 1.5 m, and
/// one box of half sides 2 to 6 m, 6 m tall, at any yaw; and the two meshes.
fn scene(seed: u64, scale: &PixelScale) -> (PickScene, Assets<Mesh>) {
    let mut dice = ViewDice::new(seed);
    let mut draw = |least: f32, most: f32| dice.between(least, most);
    let mut meshes = Assets::<Mesh>::default();
    let ball = meshes.add(Sphere::new(1.0).mesh().ico(2).unwrap()).id();
    let cube = meshes.add(Cuboid::new(2.0, 2.0, 2.0).mesh().build()).id();
    let picking = UnitPicking {
        least_radius_px: 10.0,
        ..UnitPicking::default()
    };
    let mut world = World::new();
    let mut ids = IdAllocator::default();
    let mut scene = PickScene::default();
    for index in 0..COUNT {
        let base = Vec3::new(draw(-64.0, 64.0), 0.0, draw(-64.0, 64.0));
        let (shape, place, mesh) = if index % 8 == 7 {
            let half = [draw(2.0, 6.0), draw(2.0, 6.0)];
            let yaw = draw(0.0, std::f32::consts::TAU);
            let place = Transform::from_translation(base + Vec3::Y * 3.0)
                .with_rotation(Quat::from_rotation_y(yaw))
                .with_scale(Vec3::new(half[0], 3.0, half[1]));
            let shape = UnitShape {
                radius: half[0].min(half[1]),
                height: 6.0,
                footing: Footing::Box { half, yaw },
            };
            (shape, place, cube)
        } else {
            let radius = draw(0.3, 1.5);
            let place = Transform::from_translation(base + Vec3::Y * radius)
                .with_scale(Vec3::splat(radius));
            let shape = UnitShape {
                radius,
                height: 2.0 * radius,
                footing: Footing::Circle(radius),
            };
            (shape, place, ball)
        };
        let target = Target {
            root: world.spawn_empty().id(),
            unit: Pointed {
                id: ids.allocate(),
                team: Team::new(0),
                owner: None,
            },
            tier: 0,
            base,
            shape,
            least: picking.least_at(scale, base + Vec3::Y * (shape.height / 2.0)),
        };
        let part = Part::from(PartSetup {
            place: GlobalTransform::from(place).affine(),
            center: Vec3::ZERO,
            half: Vec3::ONE,
            mesh,
            material: None,
        });
        scene.push(target, [part]);
    }
    (scene, meshes)
}

/// The client's frame of the units' picking backend, the triangles' and least radii's tests and
/// the ranking of what they meet, for one pointer over `COUNT` units, `units`: a camera 40 m up
/// and 40 m back of the square's center, of the default view on a screen 1080 pixels high, its
/// ray through the center.
pub(crate) fn picking(c: &mut Criterion) {
    let mut group = Kind::Integration.group(c, "picking", Scale::Sample);
    group.throughput(campfire_measure::elements(COUNT));
    let eye = Transform::from_xyz(0.0, 40.0, 40.0).looking_at(Vec3::ZERO, Vec3::Y);
    let scale = PixelScale::of(eye, &Projection::default(), 1080.0);
    let (scene, meshes) = scene(1, &scale);
    let materials = Assets::<FileMaterial>::default();
    let assets = PartAssets {
        meshes: &meshes,
        materials: &materials,
    };
    let ray = Ray3d::new(eye.translation, Dir3::new(-eye.translation).unwrap());
    let mut hits = Vec::with_capacity(COUNT);
    group.bench_function("units", |b| {
        b.iter(|| {
            scene.hits(black_box(ray), assets, &mut hits);
            black_box(&hits);
        });
    });
}
