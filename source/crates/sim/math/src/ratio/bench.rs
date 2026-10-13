use std::hint::black_box;

use criterion::{Criterion, Throughput};

use crate::floor_root::bench::values;
use crate::i256::bench::signed;
use crate::num::Num;
use crate::num::bench::COUNT;
use crate::ratio::Ratio;
use crate::u256::U256;
use crate::u256::bench::{divisors, squares};

/// `Ratio`'s operations over `COUNT` inputs each, its throughput the comparisons or the shares, by
/// its paths: `share`, two shares of a path over other denominators, each of 1 to 100 bits over 1
/// to 100 bits, every other one negative, compared in `I256`; `squared`, two squared distances
/// over other denominators, each a square of 1 to 72 bits over 17 to 72 bits, as the boxes' and
/// the paths' queries compare them, in `U384`; `same_den`, two such distances over one
/// denominator, by their numerators alone, as a reach's square against a corner's; and `of`, a
/// share from 0 to 1 of a number of 1 to 63 bits, as a projectile's step takes it.
pub(crate) fn ratio(c: &mut Criterion) {
    let shares = |seed: u64| -> Vec<Ratio<i128, i128>> {
        let nums = signed(seed, 100).into_iter();
        let dens = values(seed + 1, 100);
        let shares = nums.zip(dens);
        shares
            .map(|(num, den)| Ratio::new(num, den.cast_signed()))
            .collect()
    };
    let (our_shares, their_shares) = (shares(20), shares(22));
    let squared = |seed: u64| -> Vec<Ratio<U256, u128>> {
        let squares = squares(seed).into_iter().zip(divisors(seed + 1));
        squares.map(|(num, den)| Ratio::new(num, den)).collect()
    };
    let (ours, theirs) = (squared(10), squared(12));
    let dens = divisors(11);
    let over = |seed: u64| -> Vec<Ratio<U256, u128>> {
        let squares = squares(seed).into_iter().zip(&dens);
        squares.map(|(num, &den)| Ratio::new(num, den)).collect()
    };
    let (our_overs, their_overs) = (over(10), over(12));
    let parts: Vec<Ratio<i128, i128>> = values(24, 100)
        .into_iter()
        .zip(values(25, 100))
        .map(|(a, b)| Ratio::new(a.min(b).cast_signed(), a.max(b).cast_signed()))
        .collect();
    let numbers: Vec<Num> = values(26, 63)
        .into_iter()
        .map(|bits| Num::from_bits(i64::try_from(bits).expect("a value of at most 63 bits")))
        .collect();

    let mut group = c.benchmark_group("atomic/ratio");
    group.throughput(Throughput::Elements(COUNT as u64));
    group.bench_function("share", |bench| {
        bench.iter(|| {
            for (ours, theirs) in our_shares.iter().zip(&their_shares) {
                black_box(black_box(ours).cmp(theirs));
            }
        });
    });
    for (case, ours, theirs) in [
        ("squared", &ours, &theirs),
        ("same_den", &our_overs, &their_overs),
    ] {
        group.bench_function(case, |bench| {
            bench.iter(|| {
                for (ours, theirs) in ours.iter().zip(theirs) {
                    black_box(black_box(ours).cmp(theirs));
                }
            });
        });
    }
    group.bench_function("of", |bench| {
        bench.iter(|| {
            for (&part, &number) in parts.iter().zip(&numbers) {
                black_box(black_box(part).of(number));
            }
        });
    });
    group.finish();
}
