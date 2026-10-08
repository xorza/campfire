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

/// The server's tick of the reference 3v3, whole and by stage, from one load of its packages.
pub(crate) fn server(c: &mut Criterion) {
    let reference: LazyCell<Reference3v3> = LazyCell::new(Reference3v3::load);
    server_tick(c, &reference);
    server_stage(c, &reference);
}

/// A tick of the reference 3v3 as its packages hold it: its first tick, `first_3v3`, which
/// builds the sim schedule and labels the pathing grid around the map's static bodies; the worst
/// of the other ticks of a match of `TICKS` ticks, with no checkpoint and with the main thread's
/// part of a delta every tick, the copy of the state that changed, as a server sends one whenever
/// its checkpoint thread is free; and
/// the mean tick of such a match, `mean_3v3`, a whole match each iteration, its throughput the
/// ticks. A rollback re-simulates whole ticks, so it costs its depth times these.
fn server_tick(c: &mut Criterion, reference: &LazyCell<Reference3v3>) {
    let mut group = c.benchmark_group("server_tick");
    group.sample_size(10);
    group.bench_function("first_3v3", |b| {
        b.iter_custom(|matches| {
            (0..matches)
                .map(|_| timed_tick(reference.start().runner_mut()))
                .sum()
        });
    });
    group.bench_function("worst_3v3", |b| {
        b.iter_custom(|matches| {
            (0..matches)
                .map(|_| MatchCost::of_match(&mut reference.start(), timed_tick).worst)
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
                let cost = MatchCost::of_match(&mut fixed, |runner| {
                    let start = Instant::now();
                    runner.run_tick();
                    Session::changes(runner.world_mut(), &mut delta);
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
                .map(|_| MatchCost::of_match(&mut reference.start(), timed_tick).total)
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
    let stage_match = |stage: SimSet| {
        MatchCost::of_match(&mut clocked(reference), |runner| stage_tick(runner, stage))
    };
    for stage in SimSet::ALL {
        group.bench_function(id("worst", stage), |b| {
            b.iter_custom(|matches| {
                (0..matches)
                    .map(|_| stage_match(stage).worst_of_all())
                    .sum()
            });
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
/// its first tick, and the worst of the others.
#[derive(Debug, Clone, Copy)]
struct MatchCost {
    total: Duration,
    first: Duration,
    worst: Duration,
}

impl MatchCost {
    /// Plays `fixed` for `TICKS` ticks, each by `tick`, which runs it and gives its time.
    fn of_match(
        fixed: &mut FixedMatch,
        mut tick: impl FnMut(&mut Runner) -> Duration,
    ) -> MatchCost {
        MatchCost::of(|| tick(fixed.runner_mut()))
    }

    /// The cost of `TICKS` ticks, each the time `tick` gives.
    fn of(mut tick: impl FnMut() -> Duration) -> MatchCost {
        let first = tick();
        let mut cost = MatchCost {
            total: first,
            first,
            worst: Duration::ZERO,
        };
        for _ in 1..TICKS {
            let spent = tick();
            cost.total += spent;
            cost.worst = cost.worst.max(spent);
        }
        cost
    }

    /// The worst of all its ticks, the first among them.
    fn worst_of_all(self) -> Duration {
        self.first.max(self.worst)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_match_cost_splits_the_first_tick_from_the_worst_of_the_rest() {
        // Tick 0 takes 9 µs and tick n after it n % 7 µs: the 5,999 ticks after the first are
        // 857 whole rounds of 1 to 6 and 0, 21 µs each, so 17,997 µs, and 18,006 µs with the
        // first. The worst after the first is 6 µs; of all, the first's 9 µs.
        let mut n = 0;
        let cost = MatchCost::of(|| {
            let micros = if n == 0 { 9 } else { n % 7 };
            n += 1;
            Duration::from_micros(micros)
        });
        assert_eq!(n, TICKS);
        assert_eq!(cost.first, Duration::from_micros(9));
        assert_eq!(cost.worst, Duration::from_micros(6));
        assert_eq!(cost.total, Duration::from_micros(18_006));
        assert_eq!(cost.worst_of_all(), Duration::from_micros(9));
    }
}
