use std::hint::black_box;

use bevy::camera::Projection;
use bevy::ecs::world::World;
use bevy::math::{Dir3, Vec3};
use bevy::shape::Ray3d;
use bevy::transform::components::Transform;
use campfire_math::SplitMix64;
use campfire_measure::{Kind, Scale};
use campfire_sim::IdAllocator;
use campfire_values::Team;
use criterion::{Criterion, Throughput};

use crate::pointer::Pointed;
use crate::pointer::unit_picking::{Target, UnitPicking};
use crate::view::footing::Footing;
use crate::view::look::UnitShape;
use crate::view::pixel_scale::PixelScale;

/// The units of the scene, as many as a large Zero Hour battle holds.
const COUNT: usize = 1000;

/// `COUNT` units on a square of 128 m: of every eight, seven upright cylinders of radius 0.3 to
/// 1.5 m, four radii tall, and one box of half sides 2 to 6 m, 6 m tall, at any yaw.
fn scene(seed: u64) -> Vec<Target> {
    let mut words = SplitMix64::new(seed);
    let mut draw = |least: f32, most: f32| {
        let unit = (words.next_u64() >> 40) as f32 / (1_u64 << 24) as f32;
        least + unit * (most - least)
    };
    let mut world = World::new();
    let mut ids = IdAllocator::default();
    (0..COUNT)
        .map(|index| {
            let shape = if index % 8 == 7 {
                let half = [draw(2.0, 6.0), draw(2.0, 6.0)];
                UnitShape {
                    radius: half[0].min(half[1]),
                    height: 6.0,
                    footing: Footing::Box {
                        half,
                        yaw: draw(0.0, std::f32::consts::TAU),
                    },
                }
            } else {
                let radius = draw(0.3, 1.5);
                UnitShape {
                    radius,
                    height: 4.0 * radius,
                    footing: Footing::Circle(radius),
                }
            };
            Target {
                root: world.spawn_empty().id(),
                unit: Pointed {
                    id: ids.allocate(),
                    team: Team::new(0),
                    owner: None,
                },
                tier: 0,
                base: Vec3::new(draw(-64.0, 64.0), 0.0, draw(-64.0, 64.0)),
                shape,
            }
        })
        .collect()
}

/// The client's frame of the units' picking backend, the shapes' tests and the ranking of what
/// they meet, for one pointer over `COUNT` units, `units`: a camera 40 m up and 40 m back of the
/// square's center, of the default view on a screen 1080 pixels high, its ray through the center,
/// each shape widened to 10 pixels.
pub(crate) fn picking(c: &mut Criterion) {
    let mut group = Kind::Integration.group(c, "picking", Scale::Sample);
    group.throughput(Throughput::Elements(COUNT as u64));
    let targets = scene(1);
    let eye = Transform::from_xyz(0.0, 40.0, 40.0).looking_at(Vec3::ZERO, Vec3::Y);
    let scale = PixelScale::of(eye, &Projection::default(), 1080.0);
    let ray = Ray3d::new(eye.translation, Dir3::new(-eye.translation).unwrap());
    let picking = UnitPicking {
        least_radius_px: 10.0,
    };
    let mut hits = Vec::with_capacity(COUNT);
    group.bench_function("units", |b| {
        b.iter(|| {
            picking.hits(black_box(ray), &scale, black_box(&targets), &mut hits);
            black_box(&hits);
        });
    });
}
