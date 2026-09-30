use campfire_capabilities::bench;
use criterion::{criterion_group, criterion_main};

criterion_group!(benches, bench::collision);
criterion_main!(benches);
