use std::path::PathBuf;

use campfire_protocol::SessionId;

/// The paths of a client's data directory, the one place they are named: the newest receipt of
/// each session, `receipts/<session id>.receipt`, and its local server's data directory,
/// `server`, a data directory of its own with its own lock. Made from the directory's path alone,
/// so a tool reads a finished run's directory it does not hold by it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientLayout {
    root: PathBuf,
}

impl ClientLayout {
    pub const fn at(root: PathBuf) -> ClientLayout {
        ClientLayout { root }
    }

    pub(crate) fn receipts_dir(&self) -> PathBuf {
        self.root.join("receipts")
    }

    /// The newest receipt of the session `session`.
    pub(crate) fn receipt_file(&self, session: SessionId) -> PathBuf {
        self.receipts_dir().join(format!("{session}.receipt"))
    }

    /// The data directory of the client's local server.
    pub fn local_server_dir(&self) -> PathBuf {
        self.root.join("server")
    }
}
