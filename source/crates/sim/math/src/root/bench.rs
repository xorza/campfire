use std::hint::black_box;

use campfire_measure::{Kind, Scale};
use criterion::{Criterion, Throughput};

use crate::num::bench::COUNT;
use crate::rng::split_mix64::SplitMix64;
use crate::root::{FloorRoot, RoundedRoot};
use crate::simd::u64x4::U64x4;

/// The roots over `COUNT` values each, the throughput the roots: `floor_wide`, `nearest_wide` and
/// `ceil_wide` on values whose widths spread evenly from 1 to 126 bits, so each path counts: the
/// `u64` path, the `u128` guess alone, and its Newton step above 2¹⁰⁴, the nearest and the
/// ceiling root differing from the floor by their rounding alone; `nearest_cmp_wide`, the nearest
/// root's order against a value of 1 to 63 bits, with no root; `floor_narrow` and `floor_lanes`
/// on values of 1 to 64 bits, the first one at a time, the second four at a time.
pub(crate) fn root(c: &mut Criterion) {
    let wide = values(6, 126);
    let others = values(8, 63);
    let narrow: Vec<u64> = values(7, 64)
        .into_iter()
        .map(|value| u64::try_from(value).expect("a value of at most 64 bits"))
        .collect();
    let lanes: Vec<U64x4> = narrow
        .as_chunks::<4>()
        .0
        .iter()
        .map(|&lanes| U64x4::from_array(lanes))
        .collect();

    let mut group = Kind::Atomic.group(c, "root", Scale::Sample);
    group.throughput(Throughput::Elements(COUNT as u64));
    group.bench_function("floor_wide", |b| {
        b.iter(|| {
            for &value in &wide {
                black_box(black_box(value).floor_root());
            }
        });
    });
    group.bench_function("nearest_wide", |b| {
        b.iter(|| {
            for &value in &wide {
                black_box(black_box(value).nearest_root());
            }
        });
    });
    group.bench_function("ceil_wide", |b| {
        b.iter(|| {
            for &value in &wide {
                black_box(black_box(value).ceil_root());
            }
        });
    });
    group.bench_function("nearest_cmp_wide", |b| {
        b.iter(|| {
            for (&value, &other) in wide.iter().zip(&others) {
                black_box(black_box(value).nearest_root_cmp(other));
            }
        });
    });
    group.bench_function("floor_narrow", |b| {
        b.iter(|| {
            for &value in &narrow {
                black_box(black_box(u128::from(value)).floor_root());
            }
        });
    });
    group.bench_function("floor_lanes", |b| {
        b.iter(|| {
            for &value in &lanes {
                black_box(black_box(value).floor_root());
            }
        });
    });
    group.finish();
}

/// `COUNT` values from `seed`, whose widths spread evenly from 1 to `widest` bits.
pub(crate) fn values(seed: u64, widest: u32) -> Vec<u128> {
    let mut words = SplitMix64::new(seed);
    (0..COUNT)
        .map(|at| {
            let width = u32::try_from(at).expect("a count below 2³²") % widest + 1;
            let bits = (u128::from(words.next_u64()) << 64) | u128::from(words.next_u64());
            (bits >> (128 - width)) | (1 << (width - 1))
        })
        .collect()
}

/// `COUNT` values drawn from `seed`, whose widths spread evenly from 1 to `widest` bits, below
/// 128, every other one negative.
pub(crate) fn signed(seed: u64, widest: u32) -> Vec<i128> {
    debug_assert!(widest < 128, "a signed value's magnitude fits 127 bits");
    let drawn = values(seed, widest).into_iter().enumerate();
    drawn
        .map(|(at, value)| {
            let value = value.cast_signed();
            if at % 2 == 0 { value } else { -value }
        })
        .collect()
}
