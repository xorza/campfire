use std::time::Duration;

use criterion::measurement::Measurement;
use criterion::{BenchmarkGroup, SamplingMode};

/// How long one iteration of a case takes, which sets how criterion samples it: the one place a
/// group's sampling is set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    /// Microseconds an iteration.
    Sample,
    /// Milliseconds an iteration, as a match's tick or a stepper's frame takes: fewer and longer
    /// samples.
    Frame,
}

impl Scale {
    /// Sets the sampling of the cases `group` registers from here on.
    pub fn apply<M: Measurement>(self, group: &mut BenchmarkGroup<'_, M>) {
        match self {
            Self::Sample => group
                .sample_size(50)
                .sampling_mode(SamplingMode::Auto)
                .warm_up_time(Duration::from_millis(500))
                .measurement_time(Duration::from_secs(1)),
            Self::Frame => group
                .sample_size(10)
                .sampling_mode(SamplingMode::Auto)
                .warm_up_time(Duration::from_secs(1))
                .measurement_time(Duration::from_secs(3)),
        };
    }
}
