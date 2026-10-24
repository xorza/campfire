use std::hint::black_box;
use std::ops::Range;

use campfire_geometry::internals::KernelScene;
use campfire_math::{Num, Vec3};
use campfire_measure::{INPUTS, Kind, Scale};
use criterion::Criterion;

use crate::bench::WalledScene;
use crate::body_index::IndexedBody;
use crate::broadphase::internals::{indexed, scene};
use crate::route_planner::{Passage, RoutePlanner, Short, Walkable, Window};
use crate::segment::Segment;
use crate::steering::Steering;

/// The route planner's own queries on the walled scene, each over `INPUTS` inputs drawn in it, for
/// its walker: whether a line of up to 8 m from a point is blocked, `line`, as a route's smoothing
/// and a route in sight test one; the open cell nearest a point, `nearest_open`, as an unreachable
/// goal and a blocked start take one; and the place a unit of a half meter at a point steps aside
/// to, clear of a walker's passage of 6 m east through it, past the scene's units that walk as
/// units that stand, `yield_place` (04-capabilities/navigation.md, A friend yields to the nearest
/// place clear of the way).
pub(crate) fn route_planner(c: &mut Criterion) {
    let walled = WalledScene::new();
    let span = walled.span;
    let mut points = KernelScene::new(23);
    let mut point = || points.point(span);
    let lines: Vec<Segment> = (0..INPUTS)
        .map(|_| {
            let from = point();
            let to = point();
            Segment::new(from, from.step_toward(to, Num::int(8)))
        })
        .collect();
    let goals: Vec<Vec3> = (0..INPUTS).map(|_| point()).collect();
    let friends: Vec<Vec3> = (0..INPUTS).map(|_| point()).collect();
    let standing: Vec<IndexedBody> = scene(9, KernelScene::UNITS, span, 1, false)
        .iter()
        .filter(|body| body.movable)
        .map(indexed)
        .collect();
    let walkable = walled.walkable();
    let cells = &walled.cells;
    let reach = walled.grid.cell() * i64::try_from(Steering::YIELD_WINDOW).expect("a small window");
    let mut blockers = Vec::new();
    let yields: Vec<Yielding> = friends
        .iter()
        .enumerate()
        .map(|(nth, &at)| {
            let friend = IndexedBody {
                position: at,
                ..standing[nth % standing.len()]
            };
            let start = blockers.len();
            blockers.extend(
                standing
                    .iter()
                    .filter(|body| body.id != friend.id && body.position.within_ground(at, reach)),
            );
            Yielding {
                friend,
                window: Window::around(cells, cells.nearest_cell(at), Steering::YIELD_WINDOW),
                blockers: start..blockers.len(),
            }
        })
        .collect();
    let mut planner = RoutePlanner::new(&walled.cells);

    let mut group = Kind::Atomic.group(c, "route_planner", Scale::Sample);
    group.throughput(campfire_measure::elements(INPUTS));
    group.bench_function("line", |bench| {
        bench.iter(|| {
            for &line in &lines {
                black_box(walkable.blocks(black_box(line)));
            }
        });
    });
    group.bench_function("nearest_open", |bench| {
        bench.iter(|| {
            let mut work = 0;
            for &goal in &goals {
                black_box(planner.nearest_open(walkable, black_box(goal), &mut work));
            }
            black_box(work);
        });
    });
    group.bench_function("yield_place", |bench| {
        bench.iter(|| {
            for yielding in &yields {
                let short = Walkable {
                    short: Some(Short {
                        window: yielding.window,
                        blockers: &blockers[yielding.blockers.clone()],
                    }),
                    ..walkable
                };
                let at = yielding.friend.position;
                let points = [at - Vec3::from((2, 0, 0)), at + Vec3::from((4, 0, 0))];
                let passage = Passage {
                    points: &points,
                    radius: walled.walker.radius,
                };
                let to = at + Vec3::from((0, 0, 1));
                black_box(planner.yield_place(short, yielding.friend, to, passage));
            }
        });
    });
    group.finish();
}

/// A unit that steps aside in `yield_place`: its body, its window of cells, and the units that
/// stand near it, by their places in the case's buffer of them.
#[derive(Debug)]
struct Yielding {
    friend: IndexedBody,
    window: Window,
    blockers: Range<usize>,
}
