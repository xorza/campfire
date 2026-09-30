use campfire_protocol::bench;
use criterion::{criterion_group, criterion_main};

criterion_group!(benches, bench::chain_head_signature);
criterion_main!(benches);
