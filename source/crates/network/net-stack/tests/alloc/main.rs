//! Per-frame allocation audits of the whole stack on the stepper: each fixture drives one workload
//! through `campfire-measure`'s audit, which fails on the first frame over its budget and prints
//! where that frame allocated. The allocator counts every thread of the process but its main one,
//! as nextest runs each test in a process of its own (11-net-stack/testing.md, One allocation
//! binary, on the workspace's audit).

use campfire_measure::CountingAllocator;

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

mod fixtures;
