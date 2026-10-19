use criterion::measurement::Measurement;
use criterion::{BenchmarkGroup, Criterion};

use crate::scale::Scale;

/// What a case times, the first part of its id `<kind>/<group>/<case>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// One stage on a made input, in the crate that owns the stage.
    Atomic,
    /// A whole system's frames or ticks: a match, or the network stack on its stepper.
    Integration,
}

impl Kind {
    /// The group `<kind>/<name>`, sampled as `scale` says.
    pub fn group<'a, M: Measurement>(
        self,
        criterion: &'a mut Criterion<M>,
        name: &str,
        scale: Scale,
    ) -> BenchmarkGroup<'a, M> {
        let kind = match self {
            Self::Atomic => "atomic",
            Self::Integration => "integration",
        };
        let mut group = criterion.benchmark_group(format!("{kind}/{name}"));
        scale.apply(&mut group);
        group
    }
}
