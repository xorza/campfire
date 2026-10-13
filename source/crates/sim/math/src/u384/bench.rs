use std::hint::black_box;

use criterion::{Criterion, Throughput};

use crate::floor_root::bench::values;
use crate::num::bench::COUNT;
use crate::u256::U256;
use crate::u384::U384;

/// `U384::product` over `COUNT` pairs, its throughput the products, by its two paths: `narrow`, a
/// `U256` within 128 bits, one product, as most squared distances are; and `wide`, one past 128
/// bits, two products and a carry. Each times a value of 1 to 128 bits.
pub(crate) fn u384(c: &mut Criterion) {
    let narrow: Vec<U256> = values(16, 128).into_iter().map(U256::from_u128).collect();
    let wide: Vec<U256> = values(17, 128)
        .into_iter()
        .zip(values(18, 128))
        .map(|(high, low)| U256::from_halves(high, low))
        .collect();
    let by = values(19, 128);

    let mut group = c.benchmark_group("atomic/u384");
    group.throughput(Throughput::Elements(COUNT as u64));
    for (case, values) in [("narrow", &narrow), ("wide", &wide)] {
        group.bench_function(case, |bench| {
            bench.iter(|| {
                for (&value, &by) in values.iter().zip(&by) {
                    black_box(U384::product(black_box(value), by));
                }
            });
        });
    }
    group.finish();
}
