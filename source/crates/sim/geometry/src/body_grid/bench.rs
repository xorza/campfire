use std::hint::black_box;
use std::ops::ControlFlow;

use campfire_math::{Num, Vec3};
use campfire_measure::{INPUTS, Kind, Scale};
use criterion::Criterion;

use crate::body_grid::{BodyGrid, Indexed};
use crate::kernel_scene::{Density, KernelScene};
use crate::placed::Placed;

/// `BodyGrid` over the crowded scene's `KernelScene::UNITS` bodies of 0.2 to 1.19 m, as a stage
/// finds the units near a point: its build, `rebuild`, its throughput the bodies, each build of
/// bodies that moved since the one before; its refresh of the bodies it holds, which keeps its
/// index, `refresh_still`, and of bodies that moved, which builds it, `refresh_moved`; and
/// `visit_near` of `INPUTS` points within the scene with reaches of 1 to 8 m, each counting the
/// bodies it visits.
pub(crate) fn body_grid(c: &mut Criterion) {
    let span = Density::Crowded.span();
    let bodies: Vec<Indexed<(), u64>> = KernelScene::new(9)
        .bodies(KernelScene::UNITS, span, 1, false)
        .iter()
        .zip(1..)
        .map(|(body, id)| Indexed {
            id,
            key: (),
            body: Placed::new(body.point, body.shape),
        })
        .collect();
    let mut points = KernelScene::new(21);
    let near: Vec<(Vec3, Num)> = (0..INPUTS)
        .map(|_| {
            let position = points.point(span);
            let reach = Num::from_int(1 + points.below(8).cast_signed()).unwrap();
            (position, reach)
        })
        .collect();
    let moved: Vec<Indexed<(), u64>> = bodies
        .iter()
        .map(|&entry| Indexed {
            body: Placed::new(
                entry.body.position + Vec3::new(Num::EPSILON, Num::ZERO, Num::ZERO),
                entry.body.shape,
            ),
            ..entry
        })
        .collect();
    let mut grid = BodyGrid::default();

    let mut group = Kind::Atomic.group(c, "body_grid", Scale::Sample);
    group.throughput(campfire_measure::elements(KernelScene::UNITS));
    let mut turn = false;
    group.bench_function("rebuild", |bench| {
        bench.iter(|| {
            turn = !turn;
            let bodies = if turn { &moved } else { &bodies };
            grid.rebuild(black_box(bodies).iter().copied());
        });
    });
    group.bench_function("refresh_still", |bench| {
        bench.iter(|| grid.refresh(black_box(&bodies).iter().copied()));
    });
    group.bench_function("refresh_moved", |bench| {
        bench.iter(|| {
            turn = !turn;
            let bodies = if turn { &moved } else { &bodies };
            grid.refresh(black_box(bodies).iter().copied());
        });
    });
    grid.rebuild(bodies.iter().copied());
    group.throughput(campfire_measure::elements(INPUTS));
    group.bench_function("visit_near", |bench| {
        bench.iter(|| {
            let mut visited = 0;
            for &(at, reach) in &near {
                grid.visit_near(black_box(at), reach, |_| {
                    visited += 1;
                    ControlFlow::Continue(())
                });
            }
            black_box(visited);
        });
    });
    group.finish();
}
