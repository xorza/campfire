use std::hint::black_box;

use campfire_kit_moba::Order;
use campfire_math::Num;
use campfire_protocol::{InputHash, ServerSeed, SessionHeader, SessionPlayer};
use criterion::Criterion;
use lightyear::prelude::RollbackMode;

use crate::local_pair::LocalPair;

const ROOT: InputHash = InputHash::new([3; 32]);
const SERVER_SEED: ServerSeed = ServerSeed::new([9; 32]);
/// A quarter meter a tick crosses the 10 m between the two targets in 40 ticks, so a new order
/// every 40 frames keeps the hero walking and the server sending updates.
const LEG_FRAMES: u64 = 40;

/// One frame of a server and a predicting client while the hero walks, rolling back only on a
/// misprediction (none happen) and on every confirmed update: the difference is the cost of the
/// rollbacks, each as deep as the client runs ahead.
pub fn rollback(c: &mut Criterion) {
    let header = SessionHeader {
        max_input_delay: 10,
        max_input_lead: 30,
        seed_commitment: SERVER_SEED.commitment(),
        players: vec![SessionPlayer {
            chain_root: ROOT,
            seed_contribution: [4; 32],
        }],
    };
    let mut group = c.benchmark_group("rollback");
    for (name, mode) in [
        ("frame_without_rollback", RollbackMode::Check),
        ("frame_with_rollback", RollbackMode::Always),
    ] {
        let mut pair = LocalPair::new(ROOT, mode);
        pair.start_match(header.clone(), SERVER_SEED)
            .expect("the seed matches the header");
        let mut frame: u64 = 0;
        group.bench_function(name, |b| {
            b.iter(|| {
                if frame.is_multiple_of(LEG_FRAMES) {
                    let z = if frame.is_multiple_of(2 * LEG_FRAMES) {
                        5
                    } else {
                        -5
                    };
                    pair.order(Order::Move {
                        x: Num::ZERO,
                        z: Num::from_int(z).expect("a small integer"),
                    });
                }
                pair.step();
                frame += 1;
                black_box(&pair);
            });
        });
    }
    group.finish();
}
