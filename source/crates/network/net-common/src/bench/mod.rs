//! The policy every criterion case of the network stack follows; each crate's `bench::run`
//! registers its cases, in a bench target of its own.
//!
//! A case's id is `<kind>/<group>/<case>`: the kind is [`Kind`](kind::Kind), and the group is the
//! stage or the frame the case measures, in the crate whose code it is. A case builds its input in
//! its own closure, so a filtered run builds only what it times, and records wall time.

use criterion::Criterion;
use tracing::subscriber::NoSubscriber;
use tracing::{Dispatch, dispatcher};

pub mod kind;
pub mod scale;

/// Runs `cases` under a dispatcher with no subscriber: a case times the stack, not the logs a
/// subscriber would format.
pub fn without_logs(criterion: &mut Criterion, cases: impl FnOnce(&mut Criterion)) {
    dispatcher::with_default(&Dispatch::new(NoSubscriber::default()), || cases(criterion));
}

/// `n` of `thing` as an id part: `1_client`, `4_clients`.
pub fn count(n: usize, thing: &str) -> String {
    let plural = if n == 1 { "" } else { "s" };
    format!("{n}_{thing}{plural}")
}
