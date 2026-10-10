use std::hint::black_box;

use criterion::{Criterion, Throughput};

use crate::num::bench::COUNT;
use crate::product_sum::ProductSum;
use crate::rng::split_mix64::SplitMix64;

/// The terms of a sum, as many as a stat's refresh adds for a unit with a few modifiers.
const TERMS: usize = 8;

/// `ProductSum` over `COUNT` sums of `TERMS` products each, as a stat's refresh adds a modifier's
/// value times its stacks: values within ±1000, either sign, times counts within ±2⁶³, then the
/// one rounding to a number.
pub(crate) fn product_sum(c: &mut Criterion) {
    let mut words = SplitMix64::new(12);
    let terms: Vec<(i64, i128)> = (0..COUNT * TERMS)
        .map(|_| {
            let value = words.next_u64().cast_signed() % (1000 << 24);
            let count = i128::from(words.next_u64()) - (1 << 63);
            (value, count)
        })
        .collect();

    let mut group = c.benchmark_group("product_sum");
    group.throughput(Throughput::Elements(COUNT as u64));
    group.bench_function("sum", |bench| {
        bench.iter(|| {
            for sum in terms.as_chunks::<TERMS>().0 {
                let mut total = ProductSum::ZERO;
                for &(value, count) in sum {
                    total.add(black_box(value), count);
                }
                black_box(total.saturating_num());
            }
        });
    });
    group.finish();
}
