use std::hint::black_box;

use criterion::Criterion;

use crate::num::Num;
use crate::rng::rng_source::{RngSource, SegmentSeed};
use crate::rng::rng_stream::RngStream;

pub fn rng(c: &mut Criterion) {
    let mut source = RngSource::new(SegmentSeed::new([7; 32]));
    source.begin_tick(1);
    let mut entity = 0_u64;
    let mut rng = source.open(RngStream::new("bench"), u64::MAX);

    let mut group = c.benchmark_group("rng");
    group.bench_function("open", |b| {
        b.iter(|| {
            // A new entity each time, so the debug check of a test build never sees a repeat.
            entity = entity.wrapping_add(1);
            black_box(source.open(black_box(RngStream::new("ability.fire_lance")), entity))
        });
    });
    group.bench_function("next_u64", |b| b.iter(|| black_box(rng.next_u64())));
    group.bench_function("below", |b| {
        b.iter(|| black_box(rng.below(black_box(1000))));
    });
    group.bench_function("chance", |b| {
        b.iter(|| black_box(rng.chance(black_box(Num::ONE / 3))));
    });
    group.finish();
}
