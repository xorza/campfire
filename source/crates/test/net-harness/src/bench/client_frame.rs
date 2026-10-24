use campfire_math::Vec3;
use campfire_measure::{Kind, RunCost, Scale};
use campfire_net_prediction::RollbackMode;
use criterion::Criterion;

use crate::in_process_match::bench::{MATCH_TICKS, WALK_FRAMES};
use crate::in_process_match::{End, InProcessMatch, StepCost};

/// A predicting client's frame, one tick, and only the client's, each case its worst over a run,
/// the first among them, or its mean, as `RunCost::cases` times it: in the lane mode while its
/// avatar walks, rolling back only on a misprediction (none happen), `<case>_walk`, and on every
/// confirmed update, `<case>_walk_rollback`, whose difference is the cost of the rollbacks, each
/// 4 ticks deep, as far as the client runs ahead; in the MOBA 3v3 from its first wave, its hero
/// walking at its base, rolling back as a client does, only on a misprediction,
/// `<case>_walk_3v3`, on every confirmed update, `<case>_walk_rollback_3v3`, and never,
/// `<case>_walk_no_rollback_3v3`, the last two's difference the cost of the rollbacks, each a run
/// of `WALK_FRAMES` frames of one match that walks on; and the slower client's frame in a new
/// lane 1v1 of `MATCH_TICKS` ticks, `<case>_1v1`. Each link is perfect: a delayed one would make
/// the rollbacks deeper.
pub(crate) fn client_frame(c: &mut Criterion) {
    let mut group = Kind::Integration.group(c, "client_frame", Scale::Frame);
    for (scene, mode) in [
        ("walk", RollbackMode::Check),
        ("walk_rollback", RollbackMode::Always),
    ] {
        let mut local = None;
        let mut frame: u64 = 0;
        RunCost::cases(&mut group, scene, WALK_FRAMES, || {
            let local = local.get_or_insert_with(|| InProcessMatch::walking(mode));
            local.walk_run(&mut frame, &[Vec3::ZERO], |cost| cost.of(End::Client(0)))
        });
    }
    let mut local = None;
    let mut around = Vec::new();
    let mut frame: u64 = 0;
    for (scene, client) in [
        ("walk_3v3", 0),
        ("walk_rollback_3v3", 1),
        ("walk_no_rollback_3v3", 2),
    ] {
        RunCost::cases(&mut group, scene, WALK_FRAMES, || {
            let local = local.get_or_insert_with(|| {
                let local = InProcessMatch::walking_3v3();
                local.avatar_places(&mut around);
                local
            });
            local.walk_run(&mut frame, &around, |cost| cost.of(End::Client(client)))
        });
    }
    RunCost::cases(&mut group, "1v1", MATCH_TICKS, || {
        InProcessMatch::lane_1v1(StepCost::slowest_client)
    });
    group.finish();
}
