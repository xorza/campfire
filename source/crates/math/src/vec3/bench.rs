use std::hint::black_box;

use criterion::{Criterion, Throughput};

use crate::num::Num;
use crate::num::bench::split_mix;
use crate::vec3::Vec3;

const COUNT: usize = 4096;

/// Deterministic points within ±1000 m.
fn points(seed: u64) -> Vec<Vec3> {
    let mut next = split_mix(seed);
    let mut coordinate = move || Num::from_bits(next().cast_signed() % (1000 << 24));
    (0..COUNT)
        .map(|_| Vec3::new(coordinate(), coordinate(), coordinate()))
        .collect()
}

pub fn vec3(c: &mut Criterion) {
    let a = points(1);
    let b = points(2);
    let radius = Num::from_int(800).unwrap();
    let turn = Num::from_int(1).unwrap().sin_cos();

    let mut group = c.benchmark_group("vec3");
    group.throughput(Throughput::Elements(COUNT as u64));
    group.bench_function("dot", |bench| {
        bench.iter(|| {
            for (&p, &q) in a.iter().zip(&b) {
                black_box(black_box(p).checked_dot(q));
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
