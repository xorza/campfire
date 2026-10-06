use std::cell::LazyCell;
use std::hint::black_box;
use std::time::{Duration, Instant};

use campfire_common::Tick;
use campfire_sim::StateDelta;
use criterion::Criterion;

use crate::harness::reference_3v3::Reference3v3;
use crate::runner::Runner;
use crate::session::Session;

/// The ticks of a measured 3v3 match: 5 minutes at 20 Hz, the pick, then a wave every 30 s from
/// tick 2399, which fight their lanes' towers.
const TICKS: u64 = 6000;
/// How often the checkpointed match copies its changed state, in ticks.
const CHECKPOINT_EVERY: u64 = 100;

/// A tick of the reference 3v3 as its packages hold it: the mean over a match of `TICKS` ticks,
/// and the worst tick of each such match, with no checkpoint and with the main thread's part of
/// a checkpoint every `CHECKPOINT_EVERY` ticks, the copy of the state that changed. A rollback
/// re-simulates whole ticks, so it costs its depth times these.
pub(crate) fn tick_3v3(c: &mut Criterion) {
    let reference = LazyCell::new(Reference3v3::load);
    let mut group = c.benchmark_group("tick_3v3");
    group.sample_size(10);
    let mut fixed = None;
    group.bench_function("mean", |b| {
        let fixed = fixed.get_or_insert_with(|| reference.start());
        b.iter_custom(|ticks| {
            let mut spent = Duration::ZERO;
            for _ in 0..ticks {
                if fixed.runner().log().next_tick() == Tick::new(TICKS) {
                    *fixed = reference.start();
                }
                spent += timed_tick(fixed.runner_mut());
            }
            spent
        });
    });
    group.bench_function("worst_of_a_match", |b| {
        b.iter_custom(|matches| {
            let mut worst_sum = Duration::ZERO;
            for _ in 0..matches {
                let mut fixed = reference.start();
                let worst = (0..TICKS)
                    .map(|_| timed_tick(fixed.runner_mut()))
                    .max()
                    .unwrap_or_default();
                worst_sum += worst;
            }
            worst_sum
        });
    });
    group.bench_function("worst_of_a_match_checkpointed", |b| {
        let mut delta = StateDelta::default();
        b.iter_custom(|matches| {
            let mut worst_sum = Duration::ZERO;
            for _ in 0..matches {
                let mut fixed = reference.start();
                let runner = fixed.runner_mut();
                Session::track(runner.world_mut(), &mut delta);
                let worst = (1..=TICKS)
                    .map(|tick| {
                        let start = Instant::now();
                        runner.run_tick();
                        if tick % CHECKPOINT_EVERY == 0 {
                            Session::changes(runner.world_mut(), &mut delta);
                        }
                        let spent = start.elapsed();
                        black_box(&delta);
                        spent
                    })
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
