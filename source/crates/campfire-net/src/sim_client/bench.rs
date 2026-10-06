use std::hint::black_box;

use campfire_math::Vec3;
use criterion::Criterion;
use lightyear::prelude::RollbackMode;

use crate::harness::in_process_match::{InProcessMatch, StepCost};

/// A predicting client's frame, one tick, and only the client's: in the lane mode while its
/// avatar walks, rolling back only on a misprediction (none happen), `walk`, and on every
/// confirmed update, `walk_rollback`, whose difference is the cost of the rollbacks, each 4
/// ticks deep, as far as the client runs ahead; either client's worst frame in the lane 1v1,
/// `worst_1v1`; and in the reference 3v3 from its first wave, its hero walking at its base,
/// rolling back as a client does, only on a misprediction, `walk_3v3`, on every confirmed
/// update, `walk_rollback_3v3`, and never, `walk_no_rollback_3v3`, the last two's difference the
/// cost of the rollbacks. A delayed link would make the rollbacks deeper, but it would measure
/// the harness: Lightyear resends every unacked reliable message after its wall-clock round
/// trip, which a step of the manual clock hardly takes.
pub(crate) fn client_frame(c: &mut Criterion) {
    let mut group = c.benchmark_group("client_frame");
    for (name, mode) in [
        ("walk", RollbackMode::Check),
        ("walk_rollback", RollbackMode::Always),
    ] {
        let mut local = None;
        let mut frame: u64 = 0;
        let mut spent = StepCost::default();
        group.bench_function(name, |b| {
            let local = local.get_or_insert_with(|| InProcessMatch::walking(mode));
            b.iter_custom(|frames| {
                spent.clear();
                local.walk_steps(&mut frame, frames, &[Vec3::ZERO], &mut spent);
                black_box(&*local);
                spent.clients[0]
            });
        });
    }
    let mut local = None;
    let mut around = Vec::new();
    let mut frame: u64 = 0;
    let mut spent = StepCost::default();
    for (name, client) in [
        ("walk_3v3", 0),
        ("walk_rollback_3v3", 1),
        ("walk_no_rollback_3v3", 2),
    ] {
        group.bench_function(name, |b| {
            let local = local.get_or_insert_with(|| {
                let local = InProcessMatch::walking_3v3();
                local.avatar_places(&mut around);
                local
            });
            b.iter_custom(|frames| {
                spent.clear();
                local.walk_steps(&mut frame, frames, &around, &mut spent);
                black_box(&*local);
                spent.clients[client]
            });
        });
    }
    group.sample_size(10);
    group.bench_function("worst_1v1", |b| {
        b.iter_custom(|matches| {
            (0..matches)
                .map(|_| InProcessMatch::worst_1v1().worst_client())
                .sum()
        });
    });
    group.finish();
}
