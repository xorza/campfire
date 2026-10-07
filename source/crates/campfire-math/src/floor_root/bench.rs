use std::hint::black_box;

use criterion::{Criterion, Throughput};

use crate::floor_root::FloorRoot;
use crate::num::bench::{COUNT, split_mix};

/// `FloorRoot::floor_root` over `COUNT` values whose widths spread evenly from 1 to 128 bits, so
/// that each path counts: the `u64` path, the `u128` guess alone, and its Newton step above 2¹⁰⁴.
pub(crate) fn root(c: &mut Criterion) {
    let mut next = split_mix(6);
    let values: Vec<u128> = (0..COUNT)
        .map(|at| {
            let width = u32::try_from(at % 128).expect("a width below 128") + 1;
            let bits = (u128::from(next()) << 64) | u128::from(next());
            (bits >> (128 - width)) | 1 << (width - 1)
        })
        .collect();

    let mut group = c.benchmark_group("root");
    group.throughput(Throughput::Elements(COUNT as u64));
    group.bench_function("floor", |b| {
        b.iter(|| {
            for &value in &values {
                black_box(black_box(value).floor_root());
            }
        });
    });
    group.finish();
}
