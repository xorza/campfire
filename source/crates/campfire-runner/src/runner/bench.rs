use std::cell::LazyCell;
use std::hint::black_box;
use std::time::{Duration, Instant};

use campfire_sim::internals::StageClock;
use campfire_sim::{SimSet, StateDelta};
use criterion::{Criterion, Throughput};

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

/// A tick of the reference 3v3 as its packages hold it: the worst tick of a match of `TICKS`
/// ticks, with no checkpoint and with the main thread's part of a checkpoint every
/// `CHECKPOINT_EVERY` ticks, the copy of the state that changed; and the mean tick of such a
/// match, `mean_3v3`, a whole match each iteration, its throughput the ticks. A rollback
/// re-simulates whole ticks, so it costs its depth times these.
fn server_tick(c: &mut Criterion, reference: &LazyCell<Reference3v3>) {
    let mut group = c.benchmark_group("server_tick");
    group.sample_size(10);
    group.bench_function("worst_3v3", |b| {
        b.iter_custom(|matches| {
            (0..matches)
                .map(|_| MatchCost::of(&mut reference.start(), timed_tick).worst)
                .sum()
        });
    });
    group.bench_function("worst_3v3_checkpointed", |b| {
        let mut delta = StateDelta::default();
        b.iter_custom(|matches| {
            let mut worst_sum = Duration::ZERO;
            for _ in 0..matches {
                let mut fixed = reference.start();
                Session::track(fixed.runner_mut().world_mut(), &mut delta);
                let mut tick = 0;
                let cost = MatchCost::of(&mut fixed, |runner| {
                    tick += 1;
                    let start = Instant::now();
                    runner.run_tick();
                    if tick % CHECKPOINT_EVERY == 0 {
                        Session::changes(runner.world_mut(), &mut delta);
                    }
                    let spent = start.elapsed();
                    black_box(&delta);
                    spent
                });
                worst_sum += cost.worst;
            }
            worst_sum
        });
    });
    group.throughput(Throughput::Elements(TICKS));
    group.bench_function("mean_3v3", |b| {
        b.iter_custom(|matches| {
            (0..matches)
                .map(|_| MatchCost::of(&mut reference.start(), timed_tick).total)
                .sum()
        });
    });
    group.finish();
}

/// Each stage of a tick of the reference 3v3, as `StageClock`'s probes time it in the match: its
/// worst in a match of `TICKS` ticks, `worst_3v3_<stage>`, and its mean tick in such a match,
/// `mean_3v3_<stage>`, a whole match each iteration, its throughput the ticks. The stages'
/// means add up to `server_tick/mean_3v3` less the parts of a tick that run outside them: the
/// session log's, the tick's start and end, and `SimEdge::Start`.
fn server_stage(c: &mut Criterion, reference: &LazyCell<Reference3v3>) {
    let id = |statistic: &str, stage: SimSet| {
        format!("{statistic}_3v3_{}", format!("{stage:?}").to_lowercase())
    };
    let mut group = c.benchmark_group("server_stage");
    group.sample_size(10);
    let stage_match =
        |stage: SimSet| MatchCost::of(&mut clocked(reference), |runner| stage_tick(runner, stage));
    for stage in SimSet::ALL {
        group.bench_function(id("worst", stage), |b| {
            b.iter_custom(|matches| (0..matches).map(|_| stage_match(stage).worst).sum());
        });
    }
    group.throughput(Throughput::Elements(TICKS));
    for stage in SimSet::ALL {
        group.bench_function(id("mean", stage), |b| {
            b.iter_custom(|matches| (0..matches).map(|_| stage_match(stage).total).sum());
        });
    }
    group.finish();
}

/// What a match of `TICKS` ticks cost, by the time `tick` gives for each of its ticks: in all,
/// and its worst tick.
#[derive(Debug, Clone, Copy)]
struct MatchCost {
    total: Duration,
    worst: Duration,
}

impl MatchCost {
    /// Plays `fixed` for `TICKS` ticks, each by `tick`, which runs it and gives its time.
    fn of(fixed: &mut FixedMatch, mut tick: impl FnMut(&mut Runner) -> Duration) -> MatchCost {
        let mut cost = MatchCost {
            total: Duration::ZERO,
            worst: Duration::ZERO,
        };
        for _ in 0..TICKS {
            let spent = tick(fixed.runner_mut());
            cost.total += spent;
            cost.worst = cost.worst.max(spent);
        }
        cost
    }
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
