use std::path::{Path, PathBuf};

use campfire_protocol::SessionId;
use campfire_store::SecretFile;

/// The paths of a server's data directory, the one place they are named: its key, `server.nsec`,
/// from which its TLS key derives; each session's directory, `sessions/<session id>`, whose files
/// `SessionDir` names; and each ended session's log, `logs/<session id>.campfire-log`. Made from
/// the directory's path alone, so a tool reads a finished run's directory it does not hold by it,
/// with no lock to take and no access to check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerLayout {
    root: PathBuf,
}

impl ServerLayout {
    pub const fn at(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    /// The server's secret key.
    pub fn key_file(&self) -> SecretFile {
        SecretFile::at(self.root.join("server.nsec"))
    }

    pub(crate) fn sessions_dir(&self) -> PathBuf {
        self.root.join("sessions")
    }

    /// The directory of the session of `session_id`.
    pub(crate) fn session_dir(&self, session_id: SessionId) -> PathBuf {
        self.sessions_dir().join(session_id.to_string())
    }

    pub(crate) fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    /// Where the log of the session of `session_id` is published once it ends.
    pub fn published_log(&self, session_id: SessionId) -> PathBuf {
        self.logs_dir().join(format!("{session_id}.campfire-log"))
    }
}
