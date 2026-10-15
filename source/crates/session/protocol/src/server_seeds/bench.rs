use std::hint::black_box;
use std::num::{NonZeroU32, NonZeroU64};

use criterion::{Criterion, Throughput};

use crate::seed_chain::SeedChain;

/// The segments of the seed chain a server's lobby opens a session with.
const SEGMENTS: NonZeroU32 = NonZeroU32::new(1024).unwrap();

/// The seeds of a server's session of the lobby's 1,024 segments: `derive`, what its session
/// derives from the chain as it starts; `segment_starts`, the seed of each segment as it starts,
/// in the server's tick, each segment an element.
pub(crate) fn server_seeds(c: &mut Criterion) {
    let chain = SeedChain::new([5; 32], SEGMENTS);
    let seeds = chain.seeds();
    let mut group = c.benchmark_group("atomic/server_seeds");
    group.sample_size(10);
    group.bench_function("derive", |b| {
        b.iter(|| black_box(chain).seeds());
    });
    group.throughput(Throughput::Elements(NonZeroU64::from(SEGMENTS).get()));
    group.bench_function("segment_starts", |b| {
        b.iter(|| {
            for segment in 0..SEGMENTS.get() {
                black_box(seeds.seed(black_box(segment)));
            }
        });
    });
    group.finish();
}
