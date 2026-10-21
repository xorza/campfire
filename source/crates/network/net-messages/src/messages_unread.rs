use campfire_log::LogEvent;
use serde::Deserialize;
use tracing::warn;

/// A link's messages of one type that no system read in the frame they came in, which their
/// receiver drops at the frame's end: the app lacks a reader of the type.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct MessagesUnread {
    /// The type's name.
    pub(crate) message_type: String,
}

impl LogEvent for MessagesUnread {
    const MESSAGE: &'static str = "dropped the messages no system read in their frame";

    fn log(&self) {
        warn!(message_type = %self.message_type, "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests {
    use campfire_log::internals::round_trip;

    use super::*;

    #[test]
    fn the_event_reads_back_as_it_logs() {
        round_trip(&MessagesUnread {
            message_type: "campfire_net_messages::fixtures::protocol::StringMessage".to_owned(),
        });
    }
}
