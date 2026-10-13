use std::hint::black_box;

use criterion::{Criterion, Throughput};

use crate::floor_root::bench::values;
use crate::i256::I256;
use crate::num::bench::COUNT;

/// `I256::product` over `COUNT` pairs, its throughput the products: values of 1 to 127 bits, every
/// other one negative, so the products with a sign turned count as often as the others, as a
/// share's cross products take them.
pub(crate) fn i256(c: &mut Criterion) {
    let (a, b) = (signed(14, 127), signed(15, 127));

    let mut group = c.benchmark_group("atomic/i256");
    group.throughput(Throughput::Elements(COUNT as u64));
    group.bench_function("product", |bench| {
        bench.iter(|| {
            for (&x, &y) in a.iter().zip(&b) {
                black_box(I256::product(black_box(x), y));
            }
        });
    });
    group.finish();
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
