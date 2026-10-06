use std::path::{Path, PathBuf};

use campfire_protocol::SessionId;
use campfire_store::{DataDir, DataDirError};

/// A client's data directory, which it holds locked while it runs, so no second client writes
/// what it writes, and the one place its paths are named: the newest receipt of each session,
/// `receipts/<session id>.receipt`, and its local server's data directory, `server`, a data
/// directory of its own with its own lock.
#[derive(Debug)]
pub struct ClientDir(DataDir);

impl ClientDir {
    /// The data directory at `path`, made when missing, and locked.
    pub fn open(path: &Path) -> Result<ClientDir, DataDirError> {
        DataDir::open(path).map(ClientDir)
    }

    pub(crate) fn receipts_dir(&self) -> PathBuf {
        self.0.path().join("receipts")
    }

    /// The newest receipt of the session `session`.
    pub(crate) fn receipt_file(&self, session: SessionId) -> PathBuf {
        self.receipts_dir().join(format!("{session}.receipt"))
    }

    /// The data directory of the client's local server.
    pub fn local_server_dir(&self) -> PathBuf {
        self.0.path().join("server")
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "a test makes and removes the files of its fixtures"
)]
#[cfg(test)]
mod tests;
