use crate::process::Process;
use derive_more::Display;

/// Which session of a run a session log is: the LAN match's, or the local match's.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionKind {
    #[display("the server")]
    Lan,
    #[display("the local server")]
    Local,
}

impl SessionKind {
    /// The process that wrote the session's log.
    pub(crate) const fn host(self) -> Process {
        match self {
            SessionKind::Lan => Process::ServerAgain,
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
