use std::hint::black_box;

use campfire_math::Num;
use campfire_sim::Position;
use criterion::{Criterion, Throughput};

use crate::geometry::body_box::bench::COUNT;
use crate::geometry::kernel_scene::{Density, KernelScene};
use crate::navigation::broadphase::internals::scene;
use crate::units::body_grid::{BodyGrid, Placed};

/// `BodyGrid` over the crowded scene's `KernelScene::UNITS` bodies of 0.2 to 1.19 m, as a stage
/// finds the units near a point: its build, `rebuild`, its throughput the bodies; and
/// `visit_near` of `COUNT` points within the scene with reaches of 1 to 8 m, each counting the
/// bodies it visits.
pub(crate) fn body_grid(c: &mut Criterion) {
    let span = Density::Crowded.span();
    let bodies: Vec<Placed<()>> = scene(9, KernelScene::UNITS, span, 1, false)
        .iter()
        .map(|body| Placed {
            id: body.id,
            key: (),
            at: Position::new(body.at).unwrap(),
            shape: body.shape,
        })
        .collect();
    let mut points = KernelScene::new(21);
    let near: Vec<(Position, Num)> = (0..COUNT)
        .map(|_| {
            let at = Position::new(points.point(span)).unwrap();
            let reach = Num::from_int(1 + points.below(8).cast_signed()).unwrap();
            (at, reach)
        })
        .collect();
    let mut grid = BodyGrid::default();

    let mut group = c.benchmark_group("atomic/body_grid");
    group.throughput(Throughput::Elements(KernelScene::UNITS as u64));
    group.bench_function("rebuild", |bench| {
        bench.iter(|| grid.rebuild(black_box(&bodies).iter().copied()));
    });
    grid.rebuild(bodies.iter().copied());
    group.throughput(Throughput::Elements(COUNT as u64));
    group.bench_function("visit_near", |bench| {
        bench.iter(|| {
            let mut visited = 0;
            for &(at, reach) in &near {
                grid.visit_near(black_box(at), reach, |_| visited += 1);
            }
            black_box(visited);
        });
    });
    group.finish();
}
