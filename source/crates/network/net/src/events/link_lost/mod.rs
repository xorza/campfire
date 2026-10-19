use campfire_log::{JsonText, LogEvent, LogLine};
use campfire_net_payload::UnlinkReason;
use serde::{Deserialize, Serialize};
use tracing::error;

use crate::close_code::CloseCode;

/// The client's link to the server failed or ended before the client asked to leave, for `cause`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct LinkLost {
    #[serde(deserialize_with = "LogLine::json")]
    pub cause: LinkLossCause,
}

/// Why the client's link to the server was lost (11-quic-transport.md, A link's end is a type).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkLossCause {
    /// The server closed the link, for this reason.
    ServerClosed(CloseCode),
    /// The client closed the link, for this reason.
    ClientClosed(CloseCode),
    /// The link ended as the stack tells.
    Link(UnlinkReason),
    /// The server's grace period and restore window passed while the client tried again.
    RetriesEnded,
}

impl LinkLossCause {
    /// The cause a link's unlink `reason` gives, Campfire's own codes named.
    pub(crate) fn of(reason: UnlinkReason) -> Self {
        match reason {
            UnlinkReason::ClosedByPeer(code) => {
                CloseCode::of(code).map_or(Self::Link(reason), LinkLossCause::ServerClosed)
            }
            UnlinkReason::Closed(code) => {
                CloseCode::of(code).map_or(Self::Link(reason), LinkLossCause::ClientClosed)
            }
            reason => Self::Link(reason),
        }
    }
}

impl LogEvent for LinkLost {
    const MESSAGE: &'static str = "the link to the server failed or ended";

    fn log(&self) {
        error!(cause = %JsonText(&self.cause), "{}", Self::MESSAGE);
    }
}

#[cfg(test)]
mod tests;
