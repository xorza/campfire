use campfire_log::{JsonText, LogEvent, LogLine};
use campfire_quic::LinkEnd;
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
    /// The link ended as its transport tells.
    Link(LinkEnd),
    /// An in-process link ended, which tells no end.
    InProcess,
    /// The server's grace period and restore window passed while the client tried again.
    RetriesEnded,
}

impl LinkLossCause {
    /// The cause a link's `end` gives, Campfire's own codes named; none for an in-process link.
    pub(crate) fn of(end: Option<&LinkEnd>) -> Self {
        match end {
            None => Self::InProcess,
            Some(&LinkEnd::ClosedByPeer(code)) => CloseCode::of(code).map_or(
                Self::Link(LinkEnd::ClosedByPeer(code)),
                LinkLossCause::ServerClosed,
            ),
            Some(&LinkEnd::Closed(code)) => CloseCode::of(code).map_or(
                Self::Link(LinkEnd::Closed(code)),
                LinkLossCause::ClientClosed,
            ),
            Some(&end) => Self::Link(end),
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
