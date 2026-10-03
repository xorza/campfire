use std::hint::black_box;
use std::time::{Duration, Instant};

use campfire_common::Tick;
use criterion::Criterion;

use crate::reference_3v3::Reference3v3;
use crate::runner::Runner;

/// The ticks of a measured 3v3 match: 5 minutes at 20 Hz, the pick, then a wave every 30 s from
/// tick 2399, which fight their lanes' towers.
const TICKS: u64 = 6000;

/// A tick of the reference 3v3 as its packages hold it: the mean over a match of `TICKS` ticks,
/// and the worst tick of each such match. A rollback re-simulates whole ticks, so it costs its
/// depth times these.
pub fn tick_3v3(c: &mut Criterion) {
    let reference = Reference3v3::load();
    let mut group = c.benchmark_group("tick_3v3");
    group.sample_size(10);
    let mut runner = reference.start().into_runner();
    group.bench_function("mean", |b| {
        b.iter_custom(|ticks| {
            let mut spent = Duration::ZERO;
            for _ in 0..ticks {
                if runner.log().next_tick() == Tick::new(TICKS) {
                    runner = reference.start().into_runner();
                }
                spent += timed_tick(&mut runner);
            }
            spent
        });
    });
    group.bench_function("worst_of_a_match", |b| {
        b.iter_custom(|matches| {
            let mut worst_sum = Duration::ZERO;
            for _ in 0..matches {
                let mut runner = reference.start().into_runner();
                let worst = (0..TICKS)
                    .map(|_| timed_tick(&mut runner))
                    .max()
                    .unwrap_or_default();
                worst_sum += worst;
            }
            worst_sum
        });
    });
    group.finish();
}

fn timed_tick(runner: &mut Runner) -> Duration {
    let start = Instant::now();
    runner.run_tick();
    let spent = start.elapsed();
    black_box(&runner);
    spent
}
