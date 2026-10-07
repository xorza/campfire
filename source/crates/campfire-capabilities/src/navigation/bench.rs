use std::hint::black_box;
use std::time::{Duration, Instant};

use campfire_math::{Num, Vec3};
use campfire_sim::Position;
use criterion::{Criterion, Throughput};

use crate::navigation::body_index::{BodyIndex, IndexedBody};
use crate::navigation::broadphase::Broadphase;
use crate::navigation::broadphase::internals::{scene, statics};
use crate::navigation::collider::Collider;
use crate::navigation::pathing_grid::PathingGrid;
use crate::navigation::route_planner::{RoutePlanner, Walkable};
use crate::navigation::terrain::Terrain;
use crate::navigation::walker::Walker;
use crate::navigation::wall::Wall;
use crate::units::layer::Layer;
use crate::values::bounds::Bounds;
use crate::values::grid::Grid;
use crate::values::kernel_scene::{Density, KernelScene};

/// The radii of the 3v3's units that walk, in centimeters: its creeps, its camps and its heroes.
const WALKER_RADII: [i64; 5] = [35, 40, 50, 55, 70];
/// The radius of the 3v3's heroes, in centimeters.
const HERO_RADIUS: i64 = 50;
/// The routes a run of the route planner's case plans, as four waves' creeps plan theirs.
const ROUTES: usize = 100;

/// The Collide stage's work for `KernelScene::UNITS` bodies, at each density, and in the crowded
/// scene with its static bodies boxes: finding the contacts, then parting them, from the same
/// scene every run.
pub(crate) fn collision(c: &mut Criterion) {
    let mut group = c.benchmark_group("collision");
    group.throughput(Throughput::Elements(KernelScene::UNITS as u64));
    let cases = Density::ALL
        .map(|density| (density.name(), density.span(), false))
        .into_iter()
        .chain([("boxes", Density::Crowded.span(), true)]);
    for (name, span, boxes) in cases {
        let bodies = scene(9, KernelScene::UNITS, span, 1, boxes);
        let index = statics(&bodies);
        let mut broadphase = Broadphase::default();
        let mut colliders = bodies.clone();
        group.bench_function(name, |bench| {
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
    let mut statics = scene_statics(span);
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
    let mut grid = PathingGrid::new(scene_cells(span), walkers, &Terrain::default());
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

/// The Move stage's planning of long routes, as a wave's creeps plan theirs: `ROUTES` routes
/// for a walker of the 3v3's heroes' size, from the south end of the spread scene to its north
/// end, between points drawn along each, around the scene's static bodies and across the walls
/// that split it into two lanes and a jungle, on a grid of half-meter cells. Its throughput is
/// the planner's units of work, which a tick's limit counts, so criterion's rate is the time of
/// one.
pub(crate) fn route_planner(c: &mut Criterion) {
    let span = Density::Spread.span();
    let cells = scene_cells(span);
    let walls: Vec<Wall> = KernelScene::walls(span)
        .into_iter()
        .map(|area| Wall {
            layer: Layer::FIRST,
            area,
        })
        .collect();
    let walker = Walker {
        layer: Layer::FIRST,
        radius: KernelScene::centimeters(HERO_RADIUS),
    };
    let mut grid = PathingGrid::new(cells, vec![walker], &Terrain::new(&cells, &walls));
    let mut index = BodyIndex::new(walker.radius);
    index.update(&scene_statics(span));
    grid.update(&index);
    let walkable = Walkable {
        clearance: grid.clearance(walker),
        statics: &index,
        short: None,
    };
    let mut ends = KernelScene::new(17);
    let end_at = |ends: &mut KernelScene, z: i64| {
        let x = ends.below(span * 200).cast_signed() - span.cast_signed() * 100;
        let at = Vec3::new(KernelScene::centimeters(x), Num::ZERO, Num::int(z));
        Position::new(at).unwrap()
    };
    let edge = span.cast_signed() - 1;
    let routes: Vec<[Position; 2]> = (0..ROUTES)
        .map(|_| [end_at(&mut ends, -edge), end_at(&mut ends, edge)])
        .collect();
    let mut planner = RoutePlanner::new(&cells);
    let mut waypoints = Vec::new();
    let work: u64 = routes
        .iter()
        .map(|&[start, goal]| u64::from(planner.plan(walkable, start, goal, &mut waypoints).work))
        .sum();

    let mut group = c.benchmark_group("route_planner");
    group.throughput(Throughput::Elements(work));
    group.bench_function("walled", |bench| {
        bench.iter(|| {
            planner.begin_tick();
            for &[start, goal] in &routes {
                planner.plan(walkable, start, goal, &mut waypoints);
                black_box(&waypoints);
            }
        });
    });
    group.finish();
}

/// The static bodies of the spread scene's 1,000, one in four, by stable id.
fn scene_statics(span: u64) -> Vec<IndexedBody> {
    scene(9, KernelScene::UNITS, span, 1, false)
        .iter()
        .filter(|body| !body.movable)
        .map(|body| IndexedBody {
            id: body.id,
            at: Position::new(body.at).unwrap(),
            shape: body.shape,
            layer: body.layer,
        })
        .collect()
}

/// A grid of half-meter cells over a scene of `span` meters' half side, and 2 m past it, so the
/// widest body at its edge lies within.
fn scene_cells(span: u64) -> Grid {
    let edge = Num::from_int(span.cast_signed() + 2).unwrap();
    Grid::new(Num::HALF, Bounds::new([-edge; 2], [edge; 2]).unwrap()).unwrap()
}
