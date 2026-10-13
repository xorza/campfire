use std::hint::black_box;

use campfire_math::Num;
use campfire_sim::Position;
use criterion::{Criterion, Throughput};

use crate::geometry::body_box::BodyBox;
use crate::geometry::body_box::bench::COUNT;
use crate::geometry::bounds::Bounds;
use crate::geometry::grid::Grid;
use crate::geometry::kernel_scene::KernelScene;

/// `Grid`'s runs of the cells a body blocks, over `COUNT` bodies at whole centimeters within
/// 20 m of the origin on a grid of half-meter cells over 48 m square, as the pathing grid marks a
/// body's cells for a walker of 0.5 m: a circle of 0.2 to 5 m, `spans_closer`, and a box of sides
/// 1 to 9 m at a whole degree, `box_spans_closer`. Each counts the cells of its runs.
pub(crate) fn grid(c: &mut Criterion) {
    let mut scene = KernelScene::new(5);
    let bodies: Vec<(Position, Num, BodyBox)> = (0..COUNT)
        .map(|_| {
            let mut length = |from: u64, to: u64| {
                let cm = from + scene.below(to - from + 1);
                KernelScene::centimeters(cm.cast_signed())
            };
            let radius = length(20, 500);
            let size = [length(100, 900), length(100, 900)];
            let angle = Num::from_int(scene.below(360).cast_signed()).unwrap();
            let body = BodyBox::new(size, angle).unwrap();
            (Position::new(scene.point(20)).unwrap(), radius, body)
        })
        .collect();
    let edge = Num::int(24);
    let grid = Grid::new(Num::HALF, Bounds::new([-edge; 2], [edge; 2]).unwrap()).unwrap();
    let reach = Num::HALF;

    let mut group = c.benchmark_group("atomic/grid");
    group.throughput(Throughput::Elements(COUNT as u64));
    group.bench_function("spans_closer", |bench| {
        bench.iter(|| {
            let mut cells = 0;
            for &(at, radius, _) in &bodies {
                grid.spans_closer(black_box(at), radius + reach, |run| cells += run.len());
            }
            black_box(cells);
        });
    });
    group.bench_function("box_spans_closer", |bench| {
        bench.iter(|| {
            let mut cells = 0;
            for (at, _, body) in &bodies {
                grid.box_spans_closer(black_box(*at), body, reach, |run| cells += run.len());
            }
            black_box(cells);
        });
    });
    group.finish();
}
