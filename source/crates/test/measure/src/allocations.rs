use criterion::Throughput;
use criterion::measurement::{Measurement, ValueFormatter};

use crate::counting_allocator::CountingAllocator;

/// Criterion's measure of the allocations a sample makes, as `CountingAllocator` counts them in
/// its bench binary.
#[derive(Debug)]
pub struct Allocations;

impl Measurement for Allocations {
    type Intermediate = u64;
    type Value = u64;

    fn start(&self) -> u64 {
        CountingAllocator::total()
    }

    fn end(&self, start: u64) -> u64 {
        CountingAllocator::total() - start
    }

    fn add(&self, a: &u64, b: &u64) -> u64 {
        a + b
    }

    fn zero(&self) -> u64 {
        0
    }

    fn to_f64(&self, value: &u64) -> f64 {
        *value as f64
    }

    fn formatter(&self) -> &dyn ValueFormatter {
        &AllocationsFormatter
    }
}

/// The unit of `Allocations`, unscaled.
#[derive(Debug)]
struct AllocationsFormatter;

impl ValueFormatter for AllocationsFormatter {
    fn scale_values(&self, _: f64, _: &mut [f64]) -> &'static str {
        "allocations"
    }

    fn scale_throughputs(&self, _: f64, _: &Throughput, _: &mut [f64]) -> &'static str {
        "allocations"
    }

    fn scale_for_machines(&self, _: &mut [f64]) -> &'static str {
        "allocations"
    }
}
