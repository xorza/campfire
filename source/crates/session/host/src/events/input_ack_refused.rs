use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::warn;

/// The server acknowledged the player's inputs from seq `first` on in a tick before one it gave
/// an input ahead of them: the client keeps its inputs' ticks in order, so it refuses the ack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct InputAckRefused {
    pub first: u64,
}

impl LogEvent for InputAckRefused {
    const MESSAGE: &'static str =
        "the server placed an input before one ahead of it; the client refuses its ack";

    fn log(&self) {
        warn!(first = self.first, "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests {
    use campfire_log::internals::round_trip;

    use super::*;

    #[test]
    fn the_event_reads_back_what_it_logs() {
        round_trip(&InputAckRefused { first: 9 });
    }
}
