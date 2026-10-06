use std::path::{Path, PathBuf};

use bevy_ecs::resource::Resource;
use campfire_protocol::SessionId;
use campfire_store::{DataDir, DataDirError};

/// A server's data directory, which it holds locked while it runs, so no second server writes
/// what it writes, and the one place its own paths are named: its key, `server.nsec`; its TLS
/// identity, `tls`; each session's directory, `sessions/<session id>`, whose files `SessionDir`
/// names; and each ended session's log, `logs/<session id>.campfire-log`.
#[derive(Resource, Debug)]
pub struct ServerDir(DataDir);

impl ServerDir {
    /// The data directory at `path`, made when missing, and locked.
    pub fn open(path: &Path) -> Result<ServerDir, DataDirError> {
        DataDir::open(path).map(ServerDir)
    }

    pub fn path(&self) -> &Path {
        self.0.path()
    }

    /// The server's secret key.
    pub fn key_file(&self) -> PathBuf {
        self.0.path().join("server.nsec")
    }

    /// The server's TLS identity.
    pub fn tls_file(&self) -> PathBuf {
        self.0.path().join("tls")
    }

    pub(crate) fn sessions_dir(&self) -> PathBuf {
        self.0.path().join("sessions")
    }

    /// The directory of the session `session`.
    pub(crate) fn session_dir(&self, session: SessionId) -> PathBuf {
        self.sessions_dir().join(session.to_string())
    }

    pub(crate) fn logs_dir(&self) -> PathBuf {
        self.0.path().join("logs")
    }

    /// Where the log of the session `session` is published once it ends.
    pub fn published_log(&self, session: SessionId) -> PathBuf {
        self.logs_dir().join(format!("{session}.campfire-log"))
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "a test makes and removes the files of its fixtures"
)]
#[cfg(test)]
mod tests;
