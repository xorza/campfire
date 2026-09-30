use campfire_net::bench;
use criterion::{criterion_group, criterion_main};

criterion_group!(benches, bench::rollback);
criterion_main!(benches);
