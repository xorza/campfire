use campfire_common::{Seat, Tick};
use campfire_log::{JsonText, LogEvent, LogLine};
use campfire_protocol::Applied;
use serde::{Deserialize, Serialize};
use tracing::warn;

/// The server logged an input of `seat`, stamped `stamp`, as it was to log `next_tick`, and the
/// input never takes effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub(crate) struct InputNeverApplied {
    pub seat: Seat,
    pub stamp: Tick,
    pub next_tick: Tick,
    #[serde(deserialize_with = "LogLine::json")]
    pub outcome: Unapplied,
}

/// Why a logged input never takes effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Unapplied {
    /// It arrived more than the max input delay after its stamp.
    Late,
    /// Its stamp was more than the max input lead ahead of the next tick.
    Early,
}

/// The outcome of an input that never takes effect; the tick of one that does.
impl TryFrom<Applied> for Unapplied {
    type Error = Tick;

    fn try_from(applied: Applied) -> Result<Self, Tick> {
        match applied {
            Applied::At(tick) => Err(tick),
            Applied::Late => Ok(Self::Late),
            Applied::Early => Ok(Self::Early),
        }
    }
}

impl LogEvent for InputNeverApplied {
    const MESSAGE: &'static str = "logged an input that never takes effect";

    fn log(&self) {
        warn!(
            seat = self.seat.get(),
            stamp = self.stamp.get(),
            next_tick = self.next_tick.get(),
            outcome = %JsonText(&self.outcome),
            "{}",
            Self::MESSAGE
        );
    }
}

#[cfg(test)]
mod tests {
    use campfire_log::internals::round_trip;

    use super::*;

    #[test]
    fn an_input_that_never_applies_is_late_or_early() {
        let at = Tick::new(3);
        let outcomes = [Applied::At(at), Applied::Late, Applied::Early].map(Unapplied::try_from);
        assert_eq!(
            outcomes,
            [Err(at), Ok(Unapplied::Late), Ok(Unapplied::Early)]
        );
    }

    #[test]
    fn the_event_reads_back_what_it_logs() {
        for outcome in [Unapplied::Late, Unapplied::Early] {
            round_trip(&InputNeverApplied {
                seat: Seat::new(1),
                stamp: Tick::new(20),
                next_tick: Tick::new(25),
                outcome,
            });
        }
    }
}
