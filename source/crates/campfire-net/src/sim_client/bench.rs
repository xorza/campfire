use std::hint::black_box;
use std::time::{Duration, Instant};

use campfire_capabilities::Action;
use campfire_math::Num;
use criterion::Criterion;
use lightyear::prelude::RollbackMode;

use crate::harness::in_process_match::link_model::LinkModel;
use crate::harness::in_process_match::{InProcessMatch, MatchSetup};

/// A quarter meter a tick crosses the 10 m between the two targets in 40 ticks, so a new order
/// every 40 frames keeps the avatar walking and the server sending updates.
const LEG_FRAMES: u64 = 40;
/// The ticks of the measured match.
const MATCH_TICKS: u64 = 600;

/// A predicting client's frame, one tick, and only the client's: while its avatar walks, rolling
/// back only on a misprediction (none happen), `walk`, and on every confirmed update,
/// `walk_rollback`, whose difference is the cost of the rollbacks, each 4 ticks deep, as far as
/// the client runs ahead; and either client's worst frame in each 1v1 of the lane mode, as the
/// match scenario plays it, `worst_1v1`, where the rollback of each avatar's death falls. A
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
            let local = local.get_or_insert_with(|| {
                let mut local =
                    InProcessMatch::new(MatchSetup::solo(mode, 1, InProcessMatch::SEED_CHAIN));
                local.start_match();
                local
            });
            b.iter_custom(|frames| {
                let mut spent = Duration::ZERO;
                for _ in 0..frames {
                    if frame.is_multiple_of(LEG_FRAMES) {
                        let z = if frame.is_multiple_of(2 * LEG_FRAMES) {
                            5
                        } else {
                            -5
                        };
                        local.order(
                            0,
                            Action::Move {
                                x: Num::ZERO,
                                z: Num::from_int(z).expect("a small integer"),
                            },
                        );
                    }
                    let start = Instant::now();
                    local.client_frame(0);
                    spent += start.elapsed();
                    for _ in 0..local.setup().server_frames {
                        local.server_frame();
                    }
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
            let mut worst_sum = Duration::ZERO;
            for _ in 0..matches {
                worst_sum += worst_frame_of_a_match();
            }
            worst_sum
        });
    });
    group.finish();
}

fn worst_frame_of_a_match() -> Duration {
    let mut local = InProcessMatch::new(MatchSetup::duo(
        LinkModel::PERFECT,
        InProcessMatch::SEED_CHAIN,
    ));
    local.start_match();
    local.play_by_team(InProcessMatch::SCENARIO_SCRIPTS);
    let mut worst = Duration::ZERO;
    for _ in 0..MATCH_TICKS {
        for client in 0..2 {
            let start = Instant::now();
            local.client_frame(client);
            worst = worst.max(start.elapsed());
        }
        for _ in 0..local.setup().server_frames {
            local.server_frame();
        }
    }
    black_box(&local);
    worst
}
