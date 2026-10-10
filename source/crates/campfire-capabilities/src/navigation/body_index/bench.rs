use std::hint::black_box;

use campfire_sim::Position;
use criterion::{Criterion, Throughput};

use crate::geometry::body_box::bench::COUNT;
use crate::geometry::kernel_scene::{Density, KernelScene};
use crate::navigation::broadphase::internals::{scene, statics};
use crate::navigation::segment::Segment;
use crate::navigation::walker::Walker;
use crate::units::layer::Layer;

/// `BodyIndex` over the static bodies of the crowded scene's `KernelScene::UNITS`, boxes, one in
/// four, for walkers of 0.2 to 1.19 m, as collision and steering ask it, over `COUNT` points
/// within the scene: the bodies near a walker of 0.5 m, `near`, each counted; and whether such
/// a walker's step of up to 2 m from the point meets one, `blocks`.
pub(crate) fn body_index(c: &mut Criterion) {
    let span = Density::Crowded.span();
    let index = statics(&scene(9, KernelScene::UNITS, span, 1, true));
    let walker = Walker {
        layer: Layer::FIRST,
        radius: KernelScene::centimeters(50),
    };
    let mut points = KernelScene::new(22);
    let steps: Vec<Segment> = (0..COUNT)
        .map(|_| {
            let from = points.point(span);
            let mut to = from;
            to.x += KernelScene::centimeters(points.below(400).cast_signed() - 200);
            to.z += KernelScene::centimeters(points.below(400).cast_signed() - 200);
            Segment::new(Position::new(from).unwrap(), Position::new(to).unwrap())
        })
        .collect();

    let mut group = c.benchmark_group("atomic/body_index");
    group.throughput(Throughput::Elements(COUNT as u64));
    group.bench_function("near", |bench| {
        bench.iter(|| {
            let mut met = 0;
            for step in &steps {
                let at = black_box(step.start()).get();
                index.near(walker.layer, at, walker.radius, |_| met += 1);
            }
            black_box(met);
        });
    });
    group.bench_function("blocks", |bench| {
        bench.iter(|| {
            for &step in &steps {
                black_box(index.blocks(black_box(step), walker));
            }
        });
    });
    group.finish();
}
