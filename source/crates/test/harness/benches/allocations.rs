use campfire_harness::bench::run_allocations;
use campfire_measure::{Allocations, CountingAllocator};
use criterion::{Criterion, criterion_group, criterion_main};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn counted() -> Criterion<Allocations> {
    Criterion::default().with_measurement(Allocations)
}

criterion_group! {
    name = benches;
    config = counted();
    targets = run_allocations
}
criterion_main!(benches);
