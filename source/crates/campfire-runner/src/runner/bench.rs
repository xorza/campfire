use std::cell::LazyCell;
use std::hint::black_box;
use std::time::{Duration, Instant};

use campfire_common::Tick;
use campfire_sim::internals::StageClock;
use campfire_sim::{SimSet, StateDelta};
use criterion::Criterion;

use crate::harness::fixed_match::FixedMatch;
use crate::harness::reference_3v3::Reference3v3;
use crate::runner::Runner;
use crate::session::Session;

/// The ticks of a measured 3v3 match: 5 minutes at 20 Hz, the pick, then a wave every 30 s from
/// tick 2399, which fight their lanes' towers.
const TICKS: u64 = 6000;
/// How often the checkpointed match copies its changed state, in ticks.
const CHECKPOINT_EVERY: u64 = 100;

/// The server's tick of the reference 3v3, whole and by stage, from one load of its packages.
pub(crate) fn server(c: &mut Criterion) {
    let reference: LazyCell<Reference3v3> = LazyCell::new(Reference3v3::load);
    server_tick(c, &reference);
    server_stage(c, &reference);
}

/// A tick of the reference 3v3 as its packages hold it: the mean over a match of `TICKS` ticks,
/// and the worst tick of each such match, with no checkpoint and with the main thread's part of
/// a checkpoint every `CHECKPOINT_EVERY` ticks, the copy of the state that changed. A rollback
/// re-simulates whole ticks, so it costs its depth times these.
fn server_tick(c: &mut Criterion, reference: &LazyCell<Reference3v3>) {
    let mut group = c.benchmark_group("server_tick");
    group.sample_size(10);
    let mut fixed = None;
    group.bench_function("mean_3v3", |b| {
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
    group.bench_function("worst_3v3", |b| {
        b.iter_custom(|matches| {
            let mut worst_sum = Duration::ZERO;
            for _ in 0..matches {
                let mut fixed = reference.start();
                let worst = (0..TICKS)
                    .map(|_| timed_tick(fixed.runner_mut()))
                    .max()
                    .expect("a match of `TICKS` ticks");
                worst_sum += worst;
            }
            worst_sum
        });
    });
    group.bench_function("worst_3v3_checkpointed", |b| {
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
                    .expect("a match of `TICKS` ticks");
                worst_sum += worst;
            }
            worst_sum
        });
    });
    group.finish();
}

/// Each stage of a tick of the reference 3v3, as `StageClock`'s probes time it in the match: its
/// mean over a match of `TICKS` ticks, `mean_3v3_<stage>`, and its worst in each such match,
/// `worst_3v3_<stage>`. The stages' means add up to `server_tick/mean_3v3` less the session
/// log's part of a tick, which runs outside them.
fn server_stage(c: &mut Criterion, reference: &LazyCell<Reference3v3>) {
    let id = |statistic: &str, stage: SimSet| {
        format!("{statistic}_3v3_{}", format!("{stage:?}").to_lowercase())
    };
    let mut group = c.benchmark_group("server_stage");
    group.sample_size(10);
    let mut fixed = None;
    for stage in SimSet::ALL {
        group.bench_function(id("mean", stage), |b| {
            let fixed = fixed.get_or_insert_with(|| clocked(reference));
            b.iter_custom(|ticks| {
                let mut spent = Duration::ZERO;
                for _ in 0..ticks {
                    if fixed.runner().log().next_tick() == Tick::new(TICKS) {
                        *fixed = clocked(reference);
                    }
                    spent += stage_tick(fixed.runner_mut(), stage);
                }
                spent
            });
        });
    }
    for stage in SimSet::ALL {
        group.bench_function(id("worst", stage), |b| {
            b.iter_custom(|matches| {
                let mut worst_sum = Duration::ZERO;
                for _ in 0..matches {
                    let mut fixed = clocked(reference);
                    let worst = (0..TICKS)
                        .map(|_| stage_tick(fixed.runner_mut(), stage))
                        .max()
                        .expect("a match of `TICKS` ticks");
                    worst_sum += worst;
                }
                worst_sum
            });
        });
    }
    group.finish();
}

/// A new match of `reference`, with `StageClock`'s probes in its schedule.
fn clocked(reference: &Reference3v3) -> FixedMatch {
    let mut fixed = reference.start();
    StageClock::install(fixed.runner_mut().world_mut());
    fixed
}

/// Runs a tick of `runner`, whose schedule holds `StageClock`'s probes, and gives `stage`'s time
/// in it.
fn stage_tick(runner: &mut Runner, stage: SimSet) -> Duration {
    runner.run_tick();
    let spent = runner.world().resource::<StageClock>().stage(stage);
    black_box(&runner);
    spent
}

fn timed_tick(runner: &mut Runner) -> Duration {
    let start = Instant::now();
    runner.run_tick();
    let spent = start.elapsed();
    black_box(&runner);
    spent
}
