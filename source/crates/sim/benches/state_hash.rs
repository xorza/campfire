use campfire_sim::bench;
use criterion::{criterion_group, criterion_main};

criterion_group!(benches, bench::state_hash);
criterion_main!(benches);
