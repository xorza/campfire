//! What the workspace's tests and benches measure with: the counting allocator and the audit of
//! the allocations of a hot path's frames, criterion's measure of allocations, and the one policy
//! of every bench's ids and sampling (02-engine-core.md, Allocation audits; 02-engine-core.md,
//! One sampling policy). Test support, never shipped.

mod alloc_audit;
mod alloc_trace;
mod allocations;
mod counting_allocator;
mod kind;
mod scale;

use criterion::Criterion;
use criterion::measurement::Measurement;
use tracing::subscriber::NoSubscriber;
use tracing::{Dispatch, dispatcher};

pub use crate::alloc_audit::{AllocAudit, AuditReport};
pub use crate::allocations::Allocations;
pub use crate::counting_allocator::{AllocWindow, CountingAllocator};
pub use crate::kind::Kind;
pub use crate::scale::Scale;

/// Runs `cases` under a dispatcher with no subscriber: a case times its code, not the logs a
/// subscriber would format.
pub fn without_logs<M: Measurement>(
    criterion: &mut Criterion<M>,
    cases: impl FnOnce(&mut Criterion<M>),
) {
    dispatcher::with_default(&Dispatch::new(NoSubscriber::default()), || cases(criterion));
}

/// `n` of `thing` as a part of a case's id: `1_client`, `4_clients`.
pub fn count(n: usize, thing: &str) -> String {
    let plural = if n == 1 { "" } else { "s" };
    format!("{n}_{thing}{plural}")
}
