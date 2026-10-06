use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::warn;

/// A sync of the session's journal took `took_ns` nanoseconds, longer than a healthy disk takes:
/// the slowest since the last of these. The server plays on, as a sync that returns late loses
/// nothing; whether a disk that stops answering ends the server is its host's watchdog's call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct JournalSyncSlow {
    pub took_ns: u64,
}

impl LogEvent for JournalSyncSlow {
    const MESSAGE: &'static str = "a sync of the session's journal was slow";

    fn log(&self) {
        warn!(took_ns = self.took_ns, "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests {
    use campfire_log::internals::round_trip;

    use super::*;

    #[test]
    fn the_event_reads_back_what_it_logs() {
        round_trip(&JournalSyncSlow {
            took_ns: 1_500_000_000,
        });
    }
}
