use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use campfire_common::StateHash;
use campfire_log::{LogEvent, LogLine};
use campfire_protocol::SessionId;
use serde::Deserialize;
use tracing::info;

/// The session ended as `end` says, and the server wrote its log to `file`, in its data
/// directory's `logs`, of a match that ended in the state of `hash`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SessionWritten {
    #[serde(deserialize_with = "LogLine::text")]
    pub session: SessionId,
    #[serde(deserialize_with = "LogLine::text")]
    pub end: SessionEnd,
    pub file: PathBuf,
    #[serde(deserialize_with = "LogLine::text")]
    pub hash: StateHash,
}

/// What ended a session: its match, which its mode ended, every player's leave, or its owner's
/// stop of a session that runs on, as of bots alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEnd {
    MatchEnded,
    PlayersLeft,
    Stopped,
}

impl SessionEnd {
    const fn as_str(self) -> &'static str {
        match self {
            Self::MatchEnded => "match_ended",
            Self::PlayersLeft => "players_left",
            Self::Stopped => "stopped",
        }
    }
}

impl fmt::Display for SessionEnd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A log line's text, as `Display` writes it.
impl FromStr for SessionEnd {
    type Err = UnknownEnd;

    fn from_str(text: &str) -> Result<Self, UnknownEnd> {
        [Self::MatchEnded, Self::PlayersLeft, Self::Stopped]
            .into_iter()
            .find(|end| end.as_str() == text)
            .ok_or(UnknownEnd)
    }
}

/// A text no `SessionEnd` writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("no end of a session")]
pub struct UnknownEnd;

impl LogEvent for SessionWritten {
    const MESSAGE: &'static str = "wrote the session log, which \
                                   `campfire-verifier <packages directory> <file>` replays to \
                                   the same hash";

    fn log(&self) {
        info!(
            session = %self.session,
            end = %self.end,
            file = %self.file.display(),
            hash = %self.hash,
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
        for end in [
            SessionEnd::MatchEnded,
            SessionEnd::PlayersLeft,
            SessionEnd::Stopped,
        ] {
            round_trip(&SessionWritten {
                session: SessionId::new([1; 32]),
                end,
                file: PathBuf::from("0101.campfire-log"),
                hash: "ab".repeat(32).parse().unwrap(),
            });
        }
        assert_eq!("left".parse::<SessionEnd>(), Err(UnknownEnd));
    }
}
