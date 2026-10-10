use std::hint::black_box;

use criterion::{Criterion, Throughput};

use crate::ceil_root::CeilRoot;
use crate::floor_root::bench::values;
use crate::num::bench::COUNT;

/// `CeilRoot::ceil_root` over `COUNT` values, its throughput the roots: `wide` on values whose
/// widths spread evenly from 1 to 128 bits, as `atomic/floor_root/wide` takes them, so the two
/// cases differ by the round up alone.
pub(crate) fn ceil_root(c: &mut Criterion) {
    let wide = values(6, 128);

    let mut group = c.benchmark_group("atomic/ceil_root");
    group.throughput(Throughput::Elements(COUNT as u64));
    group.bench_function("wide", |b| {
        b.iter(|| {
            for &value in &wide {
                black_box(black_box(value).ceil_root());
            }
        });
    });
    group.finish();
}
