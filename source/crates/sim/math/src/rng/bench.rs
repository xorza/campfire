use std::hint::black_box;

use campfire_common::{SegmentSeed, Tick};
use criterion::{Criterion, Throughput};

use crate::num::Num;
use crate::num::bench::COUNT;
use crate::rng::Rng;
use crate::rng::rng_stream::RngStream;
use crate::rng::split_mix64::SplitMix64;

/// The sim's random sequences, each operation `COUNT` times: opening an entity's stream, and
/// drawing from an open one a word, a value below a bound within 1000, an index below such a
/// length, a fraction, a chance, and a chance of a ratio below such a bound.
pub(crate) fn rng(c: &mut Criterion) {
    let seed = SegmentSeed::new([7; 32]);
    let tick = Tick::new(1);
    let mut words = SplitMix64::new(6);
    let bounds: Vec<u64> = (0..COUNT).map(|_| 1 + words.next_u64() % 1000).collect();
    let chances: Vec<Num> = (0..COUNT)
        .map(|_| {
            Num::from_bits(
                words
                    .next_u64()
                    .cast_signed()
                    .rem_euclid(Num::ONE.to_bits()),
            )
        })
        .collect();
    let lens: Vec<usize> = bounds
        .iter()
        .map(|&bound| usize::try_from(bound).unwrap())
        .collect();
    let mut entity = 0_u64;
    let mut rng = Rng::new(&seed, RngStream::new("bench"), u64::MAX, tick);

    let mut group = c.benchmark_group("atomic/rng");
    group.throughput(Throughput::Elements(COUNT as u64));
    group.bench_function("open", |b| {
        b.iter(|| {
            for _ in 0..COUNT {
                entity = entity.wrapping_add(1);
                let stream = black_box(RngStream::new("ability.fire_lance"));
                black_box(Rng::new(&seed, stream, entity, tick));
            }
        });
    });
    group.bench_function("next_u64", |b| {
        b.iter(|| {
            for _ in 0..COUNT {
                black_box(rng.next_u64());
            }
        });
    });
    group.bench_function("below", |b| {
        b.iter(|| {
            for &bound in &bounds {
                black_box(rng.below(black_box(bound)));
            }
        });
    });
    group.bench_function("pick", |b| {
        b.iter(|| {
            for &len in &lens {
                black_box(rng.pick(black_box(len)));
            }
        });
    });
    group.bench_function("fraction", |b| {
        b.iter(|| {
            for _ in 0..COUNT {
                black_box(rng.fraction());
            }
        });
    });
    group.bench_function("chance", |b| {
        b.iter(|| {
            for &probability in &chances {
                black_box(rng.chance(black_box(probability)));
            }
        });
    });
    group.bench_function("chance_ratio", |b| {
        b.iter(|| {
            for &denominator in &bounds {
                black_box(rng.chance_ratio(black_box(denominator / 2), denominator));
            }
        });
    });
    group.finish();
}
