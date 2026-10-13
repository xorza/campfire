use campfire_net::bench;
use criterion::{criterion_group, criterion_main};

criterion_group!(benches, bench::run);
criterion_main!(benches);
