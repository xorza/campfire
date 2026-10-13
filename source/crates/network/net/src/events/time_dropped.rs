use std::time::Duration;

use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::info;

/// A frame came `dropped_ns` nanoseconds later than the most a frame may advance, after a stall:
/// the server ran the ticks of that most alone, and the rest of the time is lost, so it now runs
/// behind the wall clock, and its clients' sync follows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct TimeDropped {
    pub dropped_ns: u64,
}

impl TimeDropped {
    /// The time a frame of `real` length drops past `bound`, the most one may advance; none for
    /// a frame within it.
    pub fn of(real: Duration, bound: Duration) -> Option<TimeDropped> {
        let dropped = real
            .checked_sub(bound)
            .filter(|dropped| !dropped.is_zero())?;
        let dropped_ns = u64::try_from(dropped.as_nanos()).unwrap_or(u64::MAX);
        Some(TimeDropped { dropped_ns })
    }
}

impl LogEvent for TimeDropped {
    const MESSAGE: &'static str = "dropped the time of a frame past the most one may advance";

    fn log(&self) {
        info!(dropped_ns = self.dropped_ns, "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests {
    use campfire_log::internals::round_trip;

    use super::*;

    #[test]
    fn a_frame_drops_only_what_passes_the_bound() {
        let ms = Duration::from_millis;
        let dropped = TimeDropped {
            dropped_ns: 1_700_000_000,
        };
        assert_eq!(TimeDropped::of(ms(2000), ms(300)), Some(dropped));
        assert_eq!(TimeDropped::of(ms(300), ms(300)), None);
        assert_eq!(TimeDropped::of(ms(30), ms(300)), None);
        round_trip(&dropped);
    }
}
