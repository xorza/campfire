use std::hint::black_box;

use criterion::Criterion;
use lightyear::prelude::RollbackMode;

use crate::harness::in_process_match::InProcessMatch;

/// The server's frame, one tick, and only the server's: while the solo avatar walks, `walk`, as
/// `client_frame/walk` plays it; and the server's worst frame in the lane 1v1, its journal
/// writing, `worst_1v1`.
pub(crate) fn server_frame(c: &mut Criterion) {
    let mut group = c.benchmark_group("server_frame");
    let mut local = None;
    let mut frame: u64 = 0;
    group.bench_function("walk", |b| {
        let local = local.get_or_insert_with(|| InProcessMatch::walking(RollbackMode::Check));
        b.iter_custom(|frames| {
            let spent = local.walk_steps(&mut frame, frames).server;
            black_box(&*local);
            spent
        });
    });
    group.sample_size(10);
    group.bench_function("worst_1v1", |b| {
        b.iter_custom(|matches| {
            (0..matches)
                .map(|_| InProcessMatch::worst_1v1().server)
                .sum()
        });
    });
    group.finish();
}
