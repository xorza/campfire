//! The benches of a whole match, which only the top crate builds: a scene of each capability's
//! stages at the scale of a large battle.

mod acquire;
mod casts;
mod combat;
mod gather;
mod navigation;
mod projectiles;
mod stats;

use std::hint::black_box;
use std::time::Instant;

use campfire_measure::RunCost;
use campfire_sim::SimWorld;
use criterion::measurement::WallTime;
use criterion::{BenchmarkGroup, Criterion, Throughput};

use crate::release::r#match::Match;

/// Runs each bench of the crate whose id criterion's filter takes.
pub fn run(c: &mut Criterion) {
    gather::gather(c);
    acquire::acquire(c);
    projectiles::projectiles(c);
    navigation::navigation(c);
    combat::combat(c);
    stats::stats(c);
    casts::casts(c);
}

/// A scene that a run plays tick by tick.
trait Ticked {
    /// Runs the scene's tick `tick` of its run, from 0.
    fn tick(&mut self, tick: u64);
}

impl Ticked for Match {
    fn tick(&mut self, _: u64) {
        self.step();
    }
}

/// Registers in `group` a run of `ticks` ticks of a new scene that `build` makes each iteration,
/// from its first tick: its worst tick, `worst_<scene>`, the first among them, and its mean tick,
/// `mean_<scene>`, each case's throughput the ticks it times (02-engine-core.md, The worst tick is
/// the budget).
fn run_cases<S: Ticked>(
    group: &mut BenchmarkGroup<'_, WallTime>,
    scene: &str,
    ticks: u64,
    build: impl Fn() -> S,
) {
    let run = || {
        let mut played = build();
        let mut tick = 0;
        RunCost::of(ticks, || {
            let start = Instant::now();
            played.tick(tick);
            let spent = start.elapsed();
            black_box(&played);
            tick += 1;
            spent
        })
    };
    group.throughput(Throughput::Elements(1));
    group.bench_function(format!("worst_{scene}"), |bench| {
        bench.iter_custom(|runs| (0..runs).map(|_| run().worst_of_all()).sum());
    });
    group.throughput(Throughput::Elements(ticks));
    group.bench_function(format!("mean_{scene}"), |bench| {
        bench.iter_custom(|runs| (0..runs).map(|_| run().total).sum());
    });
}
