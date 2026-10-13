use std::hint::black_box;

use criterion::{Criterion, Throughput};

use crate::floor_root::bench::values;
use crate::num::bench::COUNT;
use crate::rounding::Rounding;
use crate::u256::U256;

/// `U256`'s operations over `COUNT` inputs each: `product` of two values of 1 to 128 bits;
/// `div_rounded` up and to nearest, as `Collider::reaching` and `Ratio::of` take it, the square of
/// a value of 1 to 72 bits over one of 17 to 72, so the quotient fits and both the native path
/// and the long division count; and `shr_rounded` of such a square by 1 to 127 bits, to nearest,
/// as `ProductSum` and the stats' totals take it.
pub(crate) fn u256(c: &mut Criterion) {
    let (a, b) = (values(8, 128), values(9, 128));
    let (ours, our_dens) = (squares(10), divisors(11));

    let mut group = c.benchmark_group("atomic/u256");
    group.throughput(Throughput::Elements(COUNT as u64));
    group.bench_function("product", |bench| {
        bench.iter(|| {
            for (&x, &y) in a.iter().zip(&b) {
                black_box(U256::product(black_box(x), y));
            }
        });
    });
    group.bench_function("div_ceiling", |bench| {
        bench.iter(|| {
            for (&square, &divisor) in ours.iter().zip(&our_dens) {
                black_box(black_box(square).div_rounded(divisor, Rounding::Ceiling));
            }
        });
    });
    group.bench_function("div_nearest_even", |bench| {
        bench.iter(|| {
            for (&square, &divisor) in ours.iter().zip(&our_dens) {
                black_box(black_box(square).div_rounded(divisor, Rounding::NearestEven));
            }
        });
    });
    group.bench_function("shr_rounded", |bench| {
        bench.iter(|| {
            for (at, &square) in ours.iter().enumerate() {
                let shift = u32::try_from(at % 127).expect("a shift below 127") + 1;
                black_box(black_box(square).shr_rounded(shift, Rounding::NearestEven));
            }
        });
    });
    group.finish();
}

/// `COUNT` squares of values of 1 to 72 bits, drawn from `seed`: a point's squared distance in
/// bits to a box's edge or a path, times its squared length.
pub(crate) fn squares(seed: u64) -> Vec<U256> {
    let roots = values(seed, 72);
    roots
        .iter()
        .map(|&root| U256::product(root, root))
        .collect()
}

/// `COUNT` values of 17 to 72 bits, drawn from `seed`: the squared lengths the squares are over.
pub(crate) fn divisors(seed: u64) -> Vec<u128> {
    let drawn = values(seed, 56);
    drawn.into_iter().map(|value| value << 16).collect()
}
