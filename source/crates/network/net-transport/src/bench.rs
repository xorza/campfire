//! The crate's criterion cases, which `benches/net-transport.rs` runs.

use campfire_net_common::bench;
use campfire_net_common::bench::kind::Kind;
use campfire_net_common::bench::scale::Scale;
use criterion::{BatchSize, BenchmarkId, Criterion};

use campfire_net_channels::internals::compression_mode::CompressionMode;

use crate::internals::compression_case::{CompressionCase, CompressionRun};

/// Runs each case of the crate whose id criterion's filter takes.
pub fn run(criterion: &mut Criterion) {
    bench::without_logs(criterion, compression);
}

/// `integration/compression/<case>_<mode>`: one frame of each [`CompressionCase`] in each
/// [`CompressionMode`].
fn compression(criterion: &mut Criterion) {
    let mut group = Kind::Integration.group(criterion, "compression", Scale::Frame);
    for case in CompressionCase::ALL {
        for &mode in CompressionMode::ALL {
            let id = format!("{}_{}", case.name, mode.name());
            group.bench_function(BenchmarkId::from_parameter(id), |bencher| {
                bencher.iter_batched_ref(
                    || case.prepare(mode),
                    CompressionRun::frame,
                    BatchSize::LargeInput,
                );
            });
        }
    }
    group.finish();
}
