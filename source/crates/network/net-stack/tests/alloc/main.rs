//! Per-frame allocation audits of the whole stack on the stepper.
//!
//! `fixtures/` drives one workload each through [`audit::Audit`], which fails on the first frame
//! over its budget and prints where that frame allocated. The counting allocator is per-thread,
//! so the audits run in parallel.

use crate::allocator::CountingAllocator;

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

mod allocator;
mod audit;
mod fixtures;
