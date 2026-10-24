use campfire_math::Vec3;
use campfire_measure::{Kind, RunCost, Scale};
use campfire_net_prediction::RollbackMode;
use criterion::Criterion;

use crate::in_process_match::bench::{MATCH_TICKS, WALK_FRAMES};
use crate::in_process_match::{End, InProcessMatch};

/// The server's frame, one tick, and only the server's, each case its worst over a run, the
/// first among them, or its mean, as `RunCost::cases` times it: while the solo avatar walks,
/// `<case>_walk`, as `integration/client_frame/<case>_walk` plays it; while the 3v3's heroes walk
/// at their bases from its first wave, `<case>_walk_3v3`, as
/// `integration/client_frame/<case>_walk_3v3` plays it, each a run of `WALK_FRAMES` frames of
/// one match that walks on; and a new lane 1v1 of `MATCH_TICKS` ticks, its journal writing,
/// `<case>_1v1`.
pub(crate) fn server_frame(c: &mut Criterion) {
    let mut group = Kind::Integration.group(c, "server_frame", Scale::Frame);
    let mut local = None;
    let mut frame: u64 = 0;
    RunCost::cases(&mut group, "walk", WALK_FRAMES, || {
        let local = local.get_or_insert_with(|| InProcessMatch::walking(RollbackMode::Check));
        local.walk_run(&mut frame, &[Vec3::ZERO], |cost| cost.of(End::Server))
    });
    let mut local = None;
    let mut around = Vec::new();
    RunCost::cases(&mut group, "walk_3v3", WALK_FRAMES, || {
        let local = local.get_or_insert_with(|| {
            let local = InProcessMatch::walking_3v3();
            local.avatar_places(&mut around);
            local
        });
        local.walk_run(&mut frame, &around, |cost| cost.of(End::Server))
    });
    RunCost::cases(&mut group, "1v1", MATCH_TICKS, || {
        InProcessMatch::lane_1v1(|cost| cost.of(End::Server))
    });
    group.finish();
}
