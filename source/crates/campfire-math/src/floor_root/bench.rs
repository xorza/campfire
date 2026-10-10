use std::hint::black_box;

use criterion::{Criterion, Throughput};

use crate::floor_root::FloorRoot;
use crate::num::bench::COUNT;
use crate::rng::split_mix64::SplitMix64;
use crate::simd::u64x4::U64x4;

/// `FloorRoot::floor_root` over `COUNT` values each, its throughput the roots: `floor` on values
/// whose widths spread evenly from 1 to 128 bits, so that each path counts: the `u64` path, the
/// `u128` guess alone, and its Newton step above 2¹⁰⁴; `narrow` and `lanes` on the same values
/// of 1 to 64 bits, the first one at a time, the second four at a time.
pub(crate) fn root(c: &mut Criterion) {
    let wide = values(6, 128);
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

    let mut group = c.benchmark_group("atomic/floor_root");
    group.throughput(Throughput::Elements(COUNT as u64));
    group.bench_function("floor", |b| {
        b.iter(|| {
            for &value in &wide {
                black_box(black_box(value).floor_root());
            }
        });
    });
    group.bench_function("narrow", |b| {
        b.iter(|| {
            for &value in &narrow {
                black_box(black_box(u128::from(value)).floor_root());
            }
        });
    });
    group.bench_function("lanes", |b| {
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
            (bits >> (128 - width)) | 1 << (width - 1)
        })
        .collect()
}
