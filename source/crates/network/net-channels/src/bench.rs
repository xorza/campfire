//! The crate's criterion cases, which `benches/net-channels.rs` runs.

use campfire_net_common::bench;
use criterion::Criterion;

use crate::fragment_receiver;

/// Runs each case of the crate whose id criterion's filter takes.
pub fn run(criterion: &mut Criterion) {
    bench::without_logs(criterion, fragment_receiver::bench::atomic);
}
