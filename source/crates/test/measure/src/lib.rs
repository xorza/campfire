//! What the workspace's tests and benches measure with: the counting allocator and the audit of
//! the allocations of a hot path's frames, criterion's measure of allocations, the cost of a run's
//! steps, and the one policy of every bench's ids and sampling (02-engine-core.md, Allocation
//! audits; 02-engine-core.md, One sampling policy). Test support, never shipped.

mod alloc_audit;
mod alloc_trace;
mod allocations;
mod call_site;
mod counting_allocator;
mod kind;
mod run_cost;
mod scale;

use criterion::measurement::Measurement;
use criterion::{Criterion, Throughput};
use tracing::subscriber::NoSubscriber;
use tracing::{Dispatch, dispatcher};

pub use crate::alloc_audit::{AllocAudit, Allowed, AuditReport};
pub use crate::allocations::Allocations;
pub use crate::call_site::CallSite;
pub use crate::counting_allocator::{AllocWindow, CountingAllocator};
pub use crate::kind::Kind;
pub use crate::run_cost::RunCost;
pub use crate::scale::Scale;

/// Runs `cases` under a dispatcher with no subscriber: a case times its code, not the logs a
/// subscriber would format.
pub fn without_logs<M: Measurement>(
    criterion: &mut Criterion<M>,
    cases: impl FnOnce(&mut Criterion<M>),
) {
    dispatcher::with_default(&Dispatch::new(NoSubscriber::default()), || cases(criterion));
}

/// The inputs an atomic case runs its operation over (02-engine-core.md, Atomic and integration
/// cases).
pub const INPUTS: usize = 4096;

/// The throughput of a case that runs over `n` items an iteration.
pub fn elements(n: usize) -> Throughput {
    Throughput::Elements(u64::try_from(n).expect("a case's items fit u64"))
}

/// The case of `operation` over `n` of `thing`, its id's last part: `send_insert_1_client`,
/// `send_insert_4_clients` (02-engine-core.md, One scheme of case names).
pub fn case(operation: &str, n: usize, thing: &str) -> String {
    let plural = if n == 1 { "" } else { "s" };
    format!("{operation}_{n}_{thing}{plural}")
}
