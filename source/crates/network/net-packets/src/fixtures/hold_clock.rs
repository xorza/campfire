use core::time::Duration;

use bevy_platform::time::Instant as OsInstant;

use crate::endpoint_runtime::HoldTiming;

/// The time of each hold of an endpoint's lock a host's timed steps made, in order, as a bench
/// measures how long a pass or a frame keeps the other thread waiting.
#[derive(Debug, Default)]
pub struct HoldClock {
    holds: Vec<Duration>,
}

impl HoldClock {
    pub fn clear(&mut self) {
        self.holds.clear();
    }

    pub fn iter(&self) -> core::iter::Copied<core::slice::Iter<'_, Duration>> {
        self.holds.iter().copied()
    }

    /// The longest hold; none before the first.
    pub fn longest(&self) -> Option<Duration> {
        self.iter().max()
    }
}

impl<'a> IntoIterator for &'a HoldClock {
    type Item = Duration;
    type IntoIter = core::iter::Copied<core::slice::Iter<'a, Duration>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl HoldTiming for HoldClock {
    fn hold<R>(&mut self, held: impl FnOnce() -> R) -> R {
        let start = OsInstant::now();
        let result = held();
        self.holds.push(start.elapsed());
        result
    }
}
