use std::hint::black_box;
use std::time::{Duration, Instant};

use campfire_math::Num;
use campfire_sim::Position;
use criterion::{Criterion, Throughput};

use crate::navigation::body_index::{BodyIndex, IndexedBody};
use crate::navigation::broadphase::Broadphase;
use crate::navigation::broadphase::internals::{scene, statics};
use crate::navigation::collider::Collider;
use crate::navigation::pathing_grid::PathingGrid;
use crate::navigation::terrain::Terrain;
use crate::navigation::walker::Walker;
use crate::units::layer::Layer;
use crate::values::bounds::Bounds;
use crate::values::grid::Grid;
use crate::values::kernel_scene::{Density, KernelScene};

/// The radii of the 3v3's units that walk, in centimeters: its creeps, its camps and its heroes.
const WALKER_RADII: [i64; 5] = [35, 40, 50, 55, 70];

/// The Collide stage's work for `KernelScene::UNITS` bodies, at each density: finding the
/// contacts, then parting them, from the same scene every run.
pub(crate) fn collision(c: &mut Criterion) {
    let mut group = c.benchmark_group("collision");
    group.throughput(Throughput::Elements(KernelScene::UNITS as u64));
    for density in Density::ALL {
        let bodies = scene(9, KernelScene::UNITS, density.span(), 1);
        let index = statics(&bodies);
        let mut broadphase = Broadphase::default();
        let mut colliders = bodies.clone();
        group.bench_function(density.name(), |bench| {
            bench.iter(|| {
                colliders.clone_from(&bodies);
                let contacts = broadphase.contacts(black_box(&colliders), &index);
                Collider::resolve(&mut colliders, contacts);
                black_box(&colliders);
            });
        });
    }
    group.finish();
}

/// The Inputs stage's update when one static body enters, as a tower that dies or a building
/// an RTS places does: the static index takes the spread scene's static bodies with one more,
/// and the pathing grid of half-meter cells over the scene marks that body's cells for each of
/// the 3v3's walker sizes and labels again the chunks they touch. Between runs the body leaves
/// again, untimed.
pub(crate) fn pathing_grid(c: &mut Criterion) {
    let span = Density::Spread.span();
    let mut statics: Vec<IndexedBody> = scene(9, KernelScene::UNITS, span, 1)
        .iter()
        .filter(|body| !body.movable)
        .map(|body| IndexedBody {
            id: body.id,
            at: Position::new(body.at).unwrap(),
            radius: body.radius,
            layer: body.layer,
        })
        .collect();
    let with = statics.clone();
    statics.pop().expect("a scene with static bodies");
    let walkers: Vec<Walker> = WALKER_RADII
        .iter()
        .map(|&cm| Walker {
            layer: Layer::FIRST,
            radius: KernelScene::centimeters(cm),
        })
        .collect();
    let widest = walkers.iter().map(|walker| walker.radius).max().unwrap();
    let edge = Num::from_int(span.cast_signed() + 2).unwrap();
    let cells = Grid::new(Num::HALF, Bounds::new([-edge; 2], [edge; 2]).unwrap()).unwrap();
    let mut grid = PathingGrid::new(cells, walkers, &Terrain::default());
    let mut index = BodyIndex::new(widest);
    index.update(&statics);
    grid.update(&index);

    let mut group = c.benchmark_group("pathing_grid");
    group.bench_function("one", |bench| {
        bench.iter_custom(|runs| {
            let mut spent = Duration::ZERO;
            for _ in 0..runs {
                let start = Instant::now();
                index.update(&with);
                grid.update(&index);
                spent += start.elapsed();
                index.update(&statics);
                grid.update(&index);
            }
            black_box(&grid);
            spent
        });
    });
    group.finish();
}
