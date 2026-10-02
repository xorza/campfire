use campfire_net::bench;
use criterion::{criterion_group, criterion_main};

criterion_group!(benches, bench::rollback, bench::worst_client_frame);
criterion_main!(benches);
