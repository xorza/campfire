use std::hint::black_box;
use std::time::Duration;

use criterion::Criterion;
use lightyear::prelude::RollbackMode;

use crate::harness::in_process_match::InProcessMatch;

/// A predicting client's frame, one tick, and only the client's: while its avatar walks, rolling
/// back only on a misprediction (none happen), `walk`, and on every confirmed update,
/// `walk_rollback`, whose difference is the cost of the rollbacks, each 4 ticks deep, as far as
/// the client runs ahead; and either client's worst frame in the lane 1v1, `worst_1v1`. A
/// delayed link would make the rollbacks deeper, but it would measure the harness: Lightyear
/// resends every unacked reliable message after its wall-clock round trip, which a step of the
/// manual clock hardly takes.
pub(crate) fn client_frame(c: &mut Criterion) {
    let mut group = c.benchmark_group("client_frame");
    for (name, mode) in [
        ("walk", RollbackMode::Check),
        ("walk_rollback", RollbackMode::Always),
    ] {
        let mut local = None;
        let mut frame: u64 = 0;
        group.bench_function(name, |b| {
            let local = local.get_or_insert_with(|| InProcessMatch::walking(mode));
            b.iter_custom(|frames| {
                let mut spent = Duration::ZERO;
                for _ in 0..frames {
                    spent += local.walk_step(frame).client;
                    frame += 1;
                }
                black_box(&*local);
                spent
            });
        });
    }
    group.sample_size(10);
    group.bench_function("worst_1v1", |b| {
        b.iter_custom(|matches| {
            (0..matches)
                .map(|_| InProcessMatch::worst_1v1().client)
                .sum()
        });
    });
    group.finish();
}
