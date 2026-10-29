use std::hint::black_box;
use std::time::{Duration, Instant};

use bevy_ecs::entity::Entity;
use campfire_common::{PlayerSlot, Tick};
use campfire_geometry::internals::{Density, KernelScene};
use campfire_geometry::{BodyBox, Bounds, Grid, Shape};
use campfire_math::{Num, Vec3};
use campfire_measure::{Kind, Scale};
use campfire_sim::IdAllocator;
use campfire_units::{Layer, Wall};
use campfire_values::Team;
use criterion::measurement::WallTime;
use criterion::{BenchmarkGroup, Criterion, Throughput};

use crate::body_index::{self, BodyIndex, IndexedBody};
use crate::broadphase::Broadphase;
use crate::broadphase::internals::{indexed, scene, static_bodies, statics};
use crate::collider::Collider;
use crate::crowd_costs::{self, CrowdCosts, CrowdWalls, Stamp, Standing};
use crate::party::{Party, PartyKey};
use crate::pathing_grid::PathingGrid;
use crate::route::RouteFound;
use crate::route_asks::{Ask, PlanWalker, RouteAsks, RouteSearch};
use crate::route_planner::{self, RoutePlanner, Walkable};
use crate::steering;
use crate::terrain::WallGrid;
use crate::walker::Walker;
use crate::walker_kinds::WalkerKinds;

/// Runs each bench of the crate whose id criterion's filter takes.
pub fn run(c: &mut Criterion) {
    body_index::bench::body_index(c);
    crowd_costs::bench::crowd_costs(c);
    route_planner::bench::route_planner(c);
    steering::bench::steering(c);
    collision(c);
    pathing_grid(c);
    route_planner(c);
}

/// The radii of the 3v3's units that walk, in centimeters: its creeps, its camps and its heroes.
const WALKER_RADII: [i64; 5] = [35, 40, 50, 55, 70];
/// The radius of the 3v3's heroes, in centimeters.
const HERO_RADIUS: i64 = 50;
/// The routes a run of the route planner's case plans, as four waves' creeps plan theirs.
const ROUTES: usize = 100;

/// The Collide stage's work for `KernelScene::UNITS` bodies, at each density, in the crowded scene
/// with its static bodies boxes, and in that scene with its first body a static box of 1,117 by 100
/// m, a Zero Hour bridge's size, 2 m past its north edge, which every walker's query of the static
/// index meets and no walker, at most 1.19 m, touches: finding the contacts, then parting them,
/// from the same scene every run.
fn collision(c: &mut Criterion) {
    let mut group = Kind::Integration.group(c, "collision", Scale::Sample);
    group.throughput(campfire_measure::elements(KernelScene::UNITS));
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
            bodies[0].point = Vec3::new(Num::ZERO, Num::ZERO, -edge);
            bodies[0].shape = Shape::Box(BodyBox::new(sides, Num::ZERO).unwrap());
            bodies[0].movable = false;
            bodies[0].walking = false;
        }
        let index = statics(&bodies);
        let walkers: Vec<Collider> = bodies.iter().filter(|body| body.movable).copied().collect();
        let mut broadphase = Broadphase::default();
        let mut colliders = walkers.clone();
        group.bench_function(name, |bench| {
            bench.iter(|| {
                colliders.clone_from(&walkers);
                let contacts = broadphase.contacts(black_box(&colliders), &index);
                Collider::resolve(&mut colliders, contacts);
                black_box(&colliders);
            });
        });
    }
    group.finish();
}

/// The Inputs stage's update when one static body enters, `enter`, as a tower that dies or a
/// building an RTS places does: the static index takes the spread scene's static bodies with one
/// more, and the pathing grid of half-meter cells over the scene marks that body's cells for each
/// of the 3v3's walker sizes and labels again the chunks they touch. Between runs the body leaves
/// again, untimed.
fn pathing_grid(c: &mut Criterion) {
    let span = Density::Spread.span();
    let mut statics = scene_statics(span);
    let with = statics.clone();
    statics.pop().expect("a scene with static bodies");
    let walkers: WalkerKinds = WALKER_RADII
        .iter()
        .map(|&cm| Walker {
            layer: Layer::FIRST,
            radius: KernelScene::centimeters(cm),
        })
        .collect();
    let widest = walkers.iter().map(|walker| walker.radius).max().unwrap();
    let mut grid = PathingGrid::new(scene_cells(span), walkers, &WallGrid::default());
    let mut index = BodyIndex::new(widest);
    index.update(&statics);
    grid.update(&index);

    let mut group = Kind::Integration.group(c, "pathing_grid", Scale::Sample);
    group.bench_function("enter", |bench| {
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

/// The spread scene's static bodies and the walls that split it into two lanes and a jungle, as
/// the 3v3's map does, on a grid of half-meter cells, for a walker of the 3v3's heroes' size: the
/// ground a wave's routes cross.
#[derive(Debug)]
pub(crate) struct WalledScene {
    pub(crate) span: u64,
    pub(crate) cells: Grid,
    pub(crate) grid: PathingGrid,
    pub(crate) statics: BodyIndex,
    pub(crate) walker: Walker,
}

impl WalledScene {
    pub(crate) fn new() -> Self {
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
        let walkers = WalkerKinds::from([walker]);
        let mut grid = PathingGrid::new(cells, walkers, &WallGrid::new(&cells, &walls));
        let mut statics = BodyIndex::new(walker.radius);
        statics.update(&scene_statics(span));
        grid.update(&statics);
        Self {
            span,
            cells,
            grid,
            statics,
            walker,
        }
    }

    /// What the walker may cross, unpriced.
    pub(crate) fn walkable(&self) -> Walkable<'_> {
        Walkable::of(self.grid.clearance(self.walker), &self.statics)
    }

    /// `count` routes from the south end of the scene to its north end, between points drawn
    /// along each from `seed`.
    pub(crate) fn routes(&self, seed: u64, count: usize) -> Vec<[Vec3; 2]> {
        let span = self.span;
        let mut ends = KernelScene::new(seed);
        let mut end_at = |z: i64| {
            let x = ends.below(span * 200).cast_signed() - span.cast_signed() * 100;
            Vec3::new(KernelScene::centimeters(x), Num::ZERO, Num::int(z))
        };
        let edge = span.cast_signed() - 1;
        (0..count).map(|_| [end_at(-edge), end_at(edge)]).collect()
    }

    /// The scene's units that walk as units of team 0 and of team 1 in turn, by stable id, that
    /// stand where they are: a crowd a route prices or walls.
    fn crowd(&self) -> CrowdCosts {
        let walkers = WalkerKinds::from([self.walker]);
        let mut costs = CrowdCosts::new(self.cells, &walkers, []);
        let bodies = scene(9, KernelScene::UNITS, self.span, 1, false);
        costs.update(
            bodies
                .iter()
                .filter(|body| body.movable)
                .enumerate()
                .map(|(nth, body)| Stamp {
                    body: indexed(body),
                    team: Some(Team::new(u8::try_from(nth % 2).unwrap())),
                    standing: Standing::Movable,
                }),
        );
        costs
    }

    /// Times `ROUTES` units 0.6 m apart in a square round the first of `routes`' starts moving to
    /// its goal: as one group, tick after tick until each has its route, as the Move stage plans
    /// a party, a last member left alone planning by itself; and each by itself.
    fn group(
        &self,
        group: &mut BenchmarkGroup<'_, WallTime>,
        planner: &mut RoutePlanner,
        routes: &[[Vec3; 2]],
    ) {
        let [start, goal] = routes[0];
        let side = ROUTES.isqrt();
        let step = Num::int(3) / 5;
        let offset = |at: usize| {
            let from_middle = at.cast_signed() - (side / 2).cast_signed();
            step * Num::from_int(i64::try_from(from_middle).unwrap()).unwrap()
        };
        let starts: Vec<Vec3> = (0..ROUTES)
            .map(|at| {
                let x = start.x + offset(at % side);
                let z = start.z + offset(at / side);
                Vec3::new(x, start.y, z)
            })
            .collect();
        let party = Party {
            key: PartyKey::Order {
                tick: Tick::new(0),
                player_slot: PlayerSlot::new(0),
                number: 0,
            },
            goal,
        };
        let mut ids = IdAllocator::default();
        let members: Vec<Ask> = starts
            .iter()
            .map(|&at| Ask {
                tick: Tick::new(0),
                party: Some(party),
                member: PlanWalker {
                    id: ids.allocate(),
                    entity: Entity::PLACEHOLDER,
                    position: at,
                    walker: self.walker,
                    team: None,
                    walls: CrowdWalls::Priced,
                    goal,
                    into: None,
                },
            })
            .collect();
        let walkable = self.walkable();
        let mut asks = RouteAsks::default();
        let mut waypoints = Vec::new();
        group.throughput(campfire_measure::elements(ROUTES));
        group.bench_function("group", |bench| {
            bench.iter(|| {
                let mut left = members.as_slice();
                while !left.is_empty() {
                    let mut answered = 0;
                    let mut answer = |_: PlanWalker, route: &[Vec3], _: RouteFound| {
                        black_box(route);
                        answered += 1;
                    };
                    asks.read(left.iter().copied());
                    let search = RouteSearch {
                        grid: &self.grid,
                        statics: &self.statics,
                        crowd: None,
                        planner,
                    };
                    asks.plan(Some(search), &mut answer);
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

/// The Move stage's planning of long routes, as a wave's creeps plan theirs: `ROUTES` routes on
/// the walled scene. Its throughput is the planner's units of work, which a tick's limit counts, so
/// criterion's rate is the time of one: with no crowd, `walled`; priced by the scene's units of
/// team 0 that stand as a walker of team 0 prices them, `priced` (04-capabilities/navigation.md, A
/// route prices the units on it); and as a stuck walker of team 0 asks it, walled by the units of
/// team 1, `stuck` (04-capabilities/navigation.md, A stuck walker's route walls the units of other
/// teams that stand). Then `ROUTES` units in a square of 6 m round the first route's start move to
/// its goal, as one group, `group`, and each by itself, `group_alone`, timed for all of them.
fn route_planner(c: &mut Criterion) {
    let scene = WalledScene::new();
    let routes = scene.routes(17, ROUTES);
    let crowd = scene.crowd();
    let mut planner = RoutePlanner::new(&scene.cells);
    let mut waypoints = Vec::new();
    let team = Some(Team::new(0));
    let cases = [
        ("walled", None),
        (
            "priced",
            crowd.price(scene.walker, team, CrowdWalls::Priced),
        ),
        ("stuck", crowd.price(scene.walker, team, CrowdWalls::Stuck)),
    ];
    let mut group = Kind::Integration.group(c, "route_planner", Scale::Sample);
    for (name, price) in cases {
        let walkable = scene.walkable().priced(price);
        let work: u64 = routes
            .iter()
            .map(|&[start, goal]| {
                u64::from(planner.plan(walkable, start, goal, &mut waypoints).work)
            })
            .sum();
        group.throughput(Throughput::Elements(work));
        group.bench_function(name, |bench| {
            bench.iter(|| {
                planner.begin_tick();
                for &[start, goal] in &routes {
                    planner.plan(walkable, start, goal, &mut waypoints);
                    black_box(&waypoints);
                }
            });
        });
    }
    scene.group(&mut group, &mut planner, &routes);
    group.finish();
}

/// The static bodies of the spread scene's 1,000, one in four, by stable id.
fn scene_statics(span: u64) -> Vec<IndexedBody> {
    static_bodies(&scene(9, KernelScene::UNITS, span, 1, false))
}

/// A grid of half-meter cells over a scene of `span` meters' half side, and 2 m past it, so the
/// widest body at its edge lies within.
pub(crate) fn scene_cells(span: u64) -> Grid {
    let edge = Num::from_int(span.cast_signed() + 2).unwrap();
    Grid::new(Num::HALF, Bounds::new([-edge; 2], [edge; 2]).unwrap()).unwrap()
}
