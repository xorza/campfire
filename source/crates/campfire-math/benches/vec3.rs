use campfire_math::bench;
use criterion::{criterion_group, criterion_main};

criterion_group!(benches, bench::vec3);
criterion_main!(benches);
