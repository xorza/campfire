use std::hint::black_box;

use campfire_math::internals::SplitMix64;
use campfire_math::{Num, Vec3};
use criterion::{Criterion, Throughput};

use crate::body_box::BodyBox;

/// The inputs of each case, each iteration's.
pub const COUNT: usize = 4096;

/// `COUNT` boxes of sides from 1 to 9 m at any angle, each at a point within 20 m of the
/// origin, with a point within 20 m of the origin beside each.
fn scene(seed: u64) -> Vec<(BodyBox, Vec3, Vec3)> {
    let mut words = SplitMix64::new(seed);
    let one = Num::ONE.to_bits();
    let mut draw = |low: i64, span: i64| {
        Num::from_bits(low + (words.next_u64() % span.cast_unsigned()).cast_signed())
    };
    let point = |draw: &mut dyn FnMut(i64, i64) -> Num| {
        let (x, z) = (draw(-20 * one, 40 * one), draw(-20 * one, 40 * one));
        Vec3::new(x, Num::ZERO, z)
    };
    (0..COUNT)
        .map(|_| {
            let size = [draw(one, 8 * one), draw(one, 8 * one)];
            let body = BodyBox::new(size, draw(0, 360 * one)).unwrap();
            (body, point(&mut draw), point(&mut draw))
        })
        .collect()
}

/// A box's primitives over `COUNT` inputs: a point's reach of 3 m to a box, two boxes' overlap,
/// and two boxes' reach of 3 m, `reach_box`, as `Metric::reaches` takes it.
pub(crate) fn body_box(c: &mut Criterion) {
    let mut group = c.benchmark_group("atomic/body_box");
    group.throughput(Throughput::Elements(COUNT as u64));
    let points = scene(1);
    let reach = Num::int(3);
    group.bench_function("reach", |bench| {
        bench.iter(|| {
            for (body, center, at) in &points {
                black_box(body.nearest(*center, black_box(*at), reach));
            }
        });
    });
    let others = scene(2);
    group.bench_function("overlap", |bench| {
        bench.iter(|| {
            for ((body, center, _), (other, other_center, _)) in points.iter().zip(&others) {
                black_box(body.overlaps(*center, other, black_box(*other_center)));
            }
        });
    });
    group.bench_function("reach_box", |bench| {
        bench.iter(|| {
            for ((body, center, _), (other, other_center, _)) in points.iter().zip(&others) {
                black_box(body.nearest_box(*center, other, black_box(*other_center), reach));
            }
        });
    });
    group.finish();
}
