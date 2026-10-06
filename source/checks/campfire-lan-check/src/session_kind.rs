use std::fmt;

use crate::process::Process;

/// Which session of a run a session log is: the LAN match's, or the local match's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionKind {
    Lan,
    Local,
}

impl SessionKind {
    /// The process that wrote the session's log.
    pub(crate) const fn host(self) -> Process {
        match self {
            SessionKind::Lan => Process::Server,
            SessionKind::Local => Process::Local,
        }
    }

    /// The verifier that replays the session's log.
    pub(crate) const fn verifier(self) -> Process {
        match self {
            SessionKind::Lan => Process::Verifier,
            SessionKind::Local => Process::LocalVerifier,
        }
    }
}

/// The host of the session, as the subject of a sentence.
impl fmt::Display for SessionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionKind::Lan => f.write_str("the server"),
            SessionKind::Local => f.write_str("the local server"),
        }
    }
}
