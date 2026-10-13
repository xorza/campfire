use campfire_common::Tick;
use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::info;

/// The server logged the record of the checkpoint that starts segment `segment` at the boundary
/// before tick `tick`, its snapshot written.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CheckpointTaken {
    pub segment: u32,
    pub tick: Tick,
}

impl LogEvent for CheckpointTaken {
    const MESSAGE: &'static str = "took a checkpoint";

    fn log(&self) {
        info!(
            segment = self.segment,
            tick = self.tick.get(),
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
    fn the_event_reads_back_what_it_logs() {
        round_trip(&CheckpointTaken {
            segment: 2,
            tick: Tick::new(600),
        });
    }
}
