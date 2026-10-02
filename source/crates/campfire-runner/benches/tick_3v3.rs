use campfire_runner::bench;
use criterion::{criterion_group, criterion_main};

criterion_group!(benches, bench::tick_3v3);
criterion_main!(benches);
