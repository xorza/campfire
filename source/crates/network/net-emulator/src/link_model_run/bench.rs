//! `atomic/link/link_model_run/admit/…`: one packet through [`LinkModelRun::admit`] under each
//! kind of model.

use campfire_math::Num;
use campfire_measure::Kind;
use campfire_measure::Scale;
use campfire_net_core::Instant;
use core::time::Duration;
use criterion::{BenchmarkId, Criterion};
use std::hint::black_box;

use crate::link_model::{LinkModel, per_mille};
use crate::link_model_run::LinkModelRun;
use crate::rate_limit::RateLimit;

/// A 1200 B packet each millisecond, which the 2 MB/s rate limit drains in 0.6 ms, so its queue
/// stays empty.
const INTERVAL: Duration = Duration::from_millis(1);
const BYTES: usize = 1200;

pub(crate) fn atomic(criterion: &mut Criterion) {
    let models = [
        ("perfect", LinkModel::perfect()),
        (
            "jitter_and_burst_loss",
            LinkModel::default()
                .with_latency(Duration::from_millis(40))
                .with_jitter(Duration::from_millis(8))
                .with_simple_gilbert_loss(per_mille(5), Num::int(8)),
        ),
        (
            "rate_limited",
            LinkModel::perfect().with_rate(RateLimit::new(2_000_000, 64_000)),
        ),
    ];
    let mut group = Kind::Atomic.group(criterion, "admit", Scale::Sample);
    for (name, model) in models {
        group.bench_function(BenchmarkId::from_parameter(name), |bencher| {
            let mut run = LinkModelRun::new(0);
            let mut now = Instant::ZERO;
            bencher.iter(|| {
                now += INTERVAL;
                black_box(run.admit(&model, BYTES, now))
            });
        });
    }
    group.finish();
}
