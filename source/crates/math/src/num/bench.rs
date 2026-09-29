use std::hint::black_box;

use criterion::{Criterion, Throughput};

use crate::num::Num;

const COUNT: usize = 4096;

/// Deterministic inputs spread over `[−span, span)` in raw bits, from `SplitMix64`.
fn spread(seed: u64, span: i64) -> Vec<Num> {
    let mut state = seed;
    (0..COUNT)
        .map(|_| {
            state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^= z >> 31;
            Num::from_bits(z.cast_signed() % span)
        })
        .collect()
}

pub fn num(c: &mut Criterion) {
    let one = Num::ONE.to_bits();
    let small = spread(1, 1000 * one);
    let other = spread(2, 1000 * one);
    let nonzero: Vec<Num> = other
        .iter()
        .map(|&x| if x == Num::ZERO { Num::ONE } else { x })
        .collect();
    let angles = spread(3, 8 * one);
    let huge_angles = spread(4, 1 << 60);
    let near_axis = spread(5, one / 1000);

    let mut group = c.benchmark_group("num");
    group.throughput(Throughput::Elements(COUNT as u64));
    group.bench_function("mul", |b| {
        b.iter(|| {
            for (&x, &y) in small.iter().zip(&other) {
                black_box(black_box(x) * y);
            }
        });
    });
    group.bench_function("div", |b| {
        b.iter(|| {
            for (&x, &y) in small.iter().zip(&nonzero) {
                black_box(black_box(x) / y);
            }
        });
    });
    group.bench_function("sqrt", |b| {
        b.iter(|| {
            for &x in &small {
                black_box(black_box(Num::from_bits(x.to_bits().abs())).sqrt());
            }
        });
    });
    group.bench_function("sin_cos", |b| {
        b.iter(|| {
            for &x in &angles {
                black_box(black_box(x).sin_cos());
            }
        });
    });
    group.bench_function("sin_cos_huge", |b| {
        b.iter(|| {
            for &x in &huge_angles {
                black_box(black_box(x).sin_cos());
            }
        });
    });
    group.bench_function("atan2", |b| {
        b.iter(|| {
            for (&y, &x) in small.iter().zip(&other) {
                black_box(black_box(y).atan2(x));
            }
        });
    });
    group.bench_function("atan2_near_axis", |b| {
        b.iter(|| {
            for (&y, &x) in near_axis.iter().zip(&other) {
                black_box(black_box(y).atan2(x));
            }
        });
    });
    group.finish();
}
