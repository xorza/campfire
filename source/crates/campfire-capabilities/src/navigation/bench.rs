use std::hint::black_box;
use std::time::{Duration, Instant};

use bevy_ecs::entity::Entity;
use campfire_common::{PlayerSlot, Tick};
use campfire_math::{Num, Vec3};
use campfire_sim::{IdAllocator, Position};
use criterion::measurement::WallTime;
use criterion::{BenchmarkGroup, Criterion, Throughput};

use crate::geometry::body_box::BodyBox;
use crate::geometry::bounds::Bounds;
use crate::geometry::grid::Grid;
use crate::geometry::kernel_scene::{Density, KernelScene};
use crate::geometry::shape::Shape;
use crate::navigation::body_index::{BodyIndex, IndexedBody};
use crate::navigation::broadphase::Broadphase;
use crate::navigation::broadphase::internals::{scene, statics};
use crate::navigation::collider::Collider;
use crate::navigation::party::{Party, PartyKey};
use crate::navigation::pathing_grid::PathingGrid;
use crate::navigation::route_asks::{Member, RouteAsks};
use crate::navigation::route_planner::{RoutePlanner, Walkable};
use crate::navigation::terrain::Terrain;
use crate::navigation::walker::Walker;
use crate::navigation::wall::Wall;
use crate::units::layer::Layer;

/// The radii of the 3v3's units that walk, in centimeters: its creeps, its camps and its heroes.
const WALKER_RADII: [i64; 5] = [35, 40, 50, 55, 70];
/// The radius of the 3v3's heroes, in centimeters.
const HERO_RADIUS: i64 = 50;
/// The routes a run of the route planner's case plans, as four waves' creeps plan theirs.
const ROUTES: usize = 100;

/// The Collide stage's work for `KernelScene::UNITS` bodies, at each density, in the crowded
/// scene with its static bodies boxes, and in that scene with its first body a static box of
/// 1,117 by 100 m, a Zero Hour bridge's size, 2 m past its north edge, which every walker's query
/// of the static index meets and no walker, at most 1.19 m, touches: finding the contacts, then parting them,
/// from the same scene every run.
pub(crate) fn collision(c: &mut Criterion) {
    let mut group = c.benchmark_group("collision");
    group.throughput(Throughput::Elements(KernelScene::UNITS as u64));
    let crowded = Density::Crowded.span();
    let cases = Density::ALL
        .map(|density| (density.name(), density.span(), false, false))
        .into_iter()
        .chain([
            ("boxes", crowded, true, false),
            ("bridge", crowded, true, true),
        ]);
    for (name, span, boxes, bridged) in cases {
        let mut bodies = scene(9, KernelScene::UNITS, span, 1, boxes);
        if bridged {
            let sides = [Num::int(1117), Num::int(100)];
            let edge = Num::int(i64::try_from(span).unwrap() + 52);
            bodies[0].at = Vec3::new(Num::ZERO, Num::ZERO, -edge);
            bodies[0].shape = Shape::Box(BodyBox::new(sides, Num::ZERO).unwrap());
            bodies[0].movable = false;
            bodies[0].walking = false;
        }
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
/// one. Then `ROUTES` units in a square of 6 m round the first route's start move to its goal,
/// as one group, `group`, and each by itself, `group_alone`, timed for all of them.
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
    let walkable = Walkable::of(grid.clearance(walker), &index);
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
    let scene = GroupScene {
        grid: &grid,
        statics: &index,
        walker,
    };
    scene.bench(&mut group, &mut planner, &routes);
    group.finish();
}

/// The walled scene of `route_planner`, for its routes as one group.
#[derive(Debug, Clone, Copy)]
struct GroupScene<'a> {
    grid: &'a PathingGrid,
    statics: &'a BodyIndex,
    walker: Walker,
}

impl GroupScene<'_> {
    /// Times `ROUTES` units 0.6 m apart in a square round the first of `routes`' starts moving to
    /// its goal: as one group, tick after tick until each has its route, as the Move stage plans
    /// a party, a last member left alone planning by itself; and each by itself.
    fn bench(
        self,
        group: &mut BenchmarkGroup<'_, WallTime>,
        planner: &mut RoutePlanner,
        routes: &[[Position; 2]],
    ) {
        let [start, goal] = routes[0];
        let side = ROUTES.isqrt();
        let step = Num::int(3) / 5;
        let offset = |at: usize| {
            let from_middle = at.cast_signed() - (side / 2).cast_signed();
            step * Num::from_int(i64::try_from(from_middle).unwrap()).unwrap()
        };
        let starts: Vec<Position> = (0..ROUTES)
            .map(|at| {
                let start = start.get();
                let x = start.x + offset(at % side);
                let z = start.z + offset(at / side);
                Position::new(Vec3::new(x, start.y, z)).unwrap()
            })
            .collect();
        let party = Party {
            key: PartyKey::Order {
                tick: Tick::new(0),
                slot: PlayerSlot::new(0),
                number: 0,
            },
            goal,
        };
        let mut ids = IdAllocator::default();
        let members: Vec<Member> = starts
            .iter()
            .map(|&at| Member {
                id: ids.allocate(),
                entity: Entity::PLACEHOLDER,
                at,
                walker: self.walker,
                goal,
            })
            .collect();
        let walkable = Walkable::of(self.grid.clearance(self.walker), self.statics);
        let mut asks = RouteAsks::default();
        let mut waypoints = Vec::new();
        group.throughput(Throughput::Elements(ROUTES as u64));
        group.bench_function("group", |bench| {
            bench.iter(|| {
                let mut left = members.as_slice();
                while let [first, rest @ ..] = left {
                    planner.begin_tick();
                    if rest.is_empty() {
                        planner.plan(walkable, first.at, first.goal, &mut waypoints);
                        black_box(&waypoints);
                        break;
                    }
                    let mut answered = 0;
                    let mut answer = |_: Entity, route: &[Position], _: bool| {
                        black_box(route);
                        answered += 1;
                    };
                    let members = left.iter().copied();
                    let planning = asks.plan_group(
                        self.grid,
                        self.statics,
                        planner,
                        party,
                        members,
                        &mut answer,
                    );
                    black_box(planning);
                    left = &left[answered..];
                }
            });
        });
        group.bench_function("group_alone", |bench| {
            bench.iter(|| {
                planner.begin_tick();
                for &start in &starts {
                    planner.plan(walkable, start, goal, &mut waypoints);
                    black_box(&waypoints);
                }
            });
        });
    }
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
