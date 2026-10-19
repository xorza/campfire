//! The crate's criterion cases, which `benches/net-emulator.rs` runs.

use criterion::Criterion;

use crate::link_model_run;

/// Runs each case of the crate whose id criterion's filter takes.
pub fn run(criterion: &mut Criterion) {
    campfire_measure::without_logs(criterion, link_model_run::bench::atomic);
}
