use std::hint::black_box;

use criterion::{Criterion, Throughput};

use crate::num::Num;
use crate::num::bench::COUNT;
use crate::rng::split_mix64::SplitMix64;
use crate::vec3::Vec3;

/// Deterministic points within ±1000 m.
fn points(seed: u64) -> Vec<Vec3> {
    let mut words = SplitMix64::new(seed);
    let mut coordinate = move || Num::from_bits(words.next_u64().cast_signed() % (1000 << 24));
    (0..COUNT)
        .map(|_| Vec3::new(coordinate(), coordinate(), coordinate()))
        .collect()
}

/// `Vec3`'s operations, each over `COUNT` pairs of points within ±1000 m: the dot product, the
/// length, the distance, the range test, a step of a meter toward the other, as a unit moves,
/// the unit vector and the turn about the vertical.
pub(crate) fn vec3(c: &mut Criterion) {
    let a = points(1);
    let b = points(2);
    let radius = Num::from_int(800).unwrap();
    let turn = Num::from_int(1).unwrap().sin_cos();

    let mut group = c.benchmark_group("atomic/vec3");
    group.throughput(Throughput::Elements(COUNT as u64));
    group.bench_function("dot", |bench| {
        bench.iter(|| {
            for (&p, &q) in a.iter().zip(&b) {
                black_box(black_box(p).checked_dot(q));
            }
        });
    });
    group.bench_function("length", |bench| {
        bench.iter(|| {
            for &p in &a {
                black_box(black_box(p).checked_length());
            }
        });
    });
    group.bench_function("distance", |bench| {
        bench.iter(|| {
            for (&p, &q) in a.iter().zip(&b) {
                black_box(black_box(p).checked_distance(q));
            }
        });
    });
    group.bench_function("within", |bench| {
        bench.iter(|| {
            for (&p, &q) in a.iter().zip(&b) {
                black_box(black_box(p).within(q, radius));
            }
        });
    });
    group.bench_function("step_toward", |bench| {
        bench.iter(|| {
            for (&p, &q) in a.iter().zip(&b) {
                black_box(black_box(p).checked_step_toward(q, Num::ONE));
            }
        });
    });
    group.bench_function("normalized", |bench| {
        bench.iter(|| {
            for &p in &a {
                black_box(black_box(p).normalized());
            }
        });
    });
    group.bench_function("rotated_y", |bench| {
        bench.iter(|| {
            for &p in &a {
                black_box(black_box(p).checked_rotated_y(turn));
            }
        });
    });
    group.finish();
}
