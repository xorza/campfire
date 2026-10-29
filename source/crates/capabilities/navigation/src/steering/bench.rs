use std::hint::black_box;

use campfire_geometry::internals::{Density, KernelScene};
use campfire_math::{Num, Vec3};
use campfire_measure::{INPUTS, Kind, Scale};
use campfire_sim::IdAllocator;
use campfire_units::Layer;
use criterion::Criterion;

use crate::aside_depth::AsideDepth;
use crate::bench::scene_cells;
use crate::broadphase::internals::{indexed, scene, statics};
use crate::gait::Gait;
use crate::moves::Moves;
use crate::pathing_grid::PathingGrid;
use crate::route_planner::RoutePlanner;
use crate::steering::{Steered, Steering, TickBody};
use crate::terrain::WallGrid;
use crate::walker::Walker;
use crate::walker_kinds::WalkerKinds;

/// Steering in the crowded scene of `KernelScene::UNITS` bodies, those that walk walking and the
/// rest standing, idle: its read of the tick's bodies, `read`, its throughput the bodies; and its
/// steer of `INPUTS` walkers of a half meter, each at a point of the scene bound for a point 8 m
/// off, round the units that stand on the next 4 m of its way by a short route, `steer`, each with
/// the tick's work to itself (04-capabilities/navigation.md, Three kinds of avoidance, as Dota
/// 2's).
pub(crate) fn steering(c: &mut Criterion) {
    let span = Density::Crowded.span();
    let colliders = scene(9, KernelScene::UNITS, span, 1, true);
    let index = statics(&colliders);
    let bodies: Vec<TickBody> = colliders
        .iter()
        .filter(|collider| collider.movable)
        .map(|collider| TickBody {
            body: indexed(collider),
            team: None,
            gait: Gait::Step,
            walks: collider.walking,
            gathers: false,
            idle: !collider.walking,
            arrived: None,
        })
        .collect();
    let walker = Walker {
        layer: Layer::FIRST,
        radius: Num::HALF,
    };
    let cells = scene_cells(span);
    let mut grid = PathingGrid::new(cells, WalkerKinds::from([walker]), &WallGrid::default());
    grid.update(&index);
    let mut points = KernelScene::new(24);
    let mut ids = IdAllocator::default();
    let walks: Vec<[Vec3; 2]> = (0..INPUTS)
        .map(|_| {
            let from = points.point(span);
            let to = points.point(span);
            [from, from.step_toward(to, Num::int(8))]
        })
        .collect();
    let mut steering = Steering::default();
    let mut planner = RoutePlanner::new(&cells);

    let mut group = Kind::Atomic.group(c, "steering", Scale::Sample);
    group.throughput(campfire_measure::elements(bodies.len()));
    group.bench_function("read", |bench| {
        bench.iter(|| steering.read(&index, Moves::Team, black_box(bodies.iter().copied())));
    });
    group.throughput(campfire_measure::elements(INPUTS));
    group.bench_function("steer", |bench| {
        bench.iter(|| {
            for &[from, goal] in &walks {
                let steered = Steered {
                    id: ids.allocate(),
                    position: from,
                    team: None,
                    gait: Gait::Step,
                    through: false,
                    step: Num::QUARTER,
                    walker,
                    stuck: false,
                    gathering: false,
                    goal,
                    into: None,
                    depth: AsideDepth::default(),
                };
                planner.begin_tick();
                let ahead = [goal];
                black_box(steering.steer(&mut planner, &grid, &index, steered, &ahead));
            }
        });
    });
    group.finish();
}
