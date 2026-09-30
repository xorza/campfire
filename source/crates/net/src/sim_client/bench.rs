use std::hint::black_box;
use std::num::NonZeroU32;

use campfire_capabilities::Action;
use campfire_math::Num;
use campfire_protocol::SeedChain;
use criterion::Criterion;
use lightyear::prelude::RollbackMode;

use crate::local_match::{LocalMatch, MatchSetup};

const SEED_CHAIN: SeedChain = SeedChain::new([9; 32], NonZeroU32::MIN);
/// A quarter meter a tick crosses the 10 m between the two targets in 40 ticks, so a new order
/// every 40 frames keeps the hero walking and the server sending updates.
const LEG_FRAMES: u64 = 40;

/// One frame of a server and a predicting client while the hero walks, rolling back only on a
/// misprediction (none happen) and on every confirmed update: the difference is the cost of the
/// rollbacks, each as deep as the client runs ahead.
pub fn rollback(c: &mut Criterion) {
    let mut group = c.benchmark_group("rollback");
    for (name, mode) in [
        ("frame_without_rollback", RollbackMode::Check),
        ("frame_with_rollback", RollbackMode::Always),
    ] {
        let mut local = LocalMatch::new(MatchSetup::solo(mode, 1, SEED_CHAIN));
        local.start_match();
        let mut frame: u64 = 0;
        group.bench_function(name, |b| {
            b.iter(|| {
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
                local.step();
                frame += 1;
                black_box(&local);
            });
        });
    }
    group.finish();
}
