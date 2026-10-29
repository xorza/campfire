use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::warn;

use crate::timeline_switch::SwitchRejection;

/// A timeline switch of `entity`, by its bits, dropped for `rejection`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub(crate) struct SwitchRefused {
    pub(crate) entity: u64,
    pub(crate) rejection: SwitchRejection,
}

impl LogEvent for SwitchRefused {
    const MESSAGE: &'static str = "a timeline switch the entity cannot take is dropped";

    fn log(&self) {
        warn!(
            entity = self.entity,
            rejection = ?self.rejection,
            "{}",
            Self::MESSAGE
        );
    }
}

#[cfg(test)]
mod tests {
    use campfire_log::internals::round_trip;

    use super::*;

    /// The rest of the rejections read back through `an_invalid_request_is_dropped`.
    #[test]
    fn the_event_reads_back_as_it_logs() {
        round_trip(&SwitchRefused {
            entity: 4_294_967_303,
            rejection: SwitchRejection::NotPredicted,
        });
    }
}
