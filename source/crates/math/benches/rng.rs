use campfire_math::bench;
use criterion::{criterion_group, criterion_main};

criterion_group!(benches, bench::rng);
criterion_main!(benches);
