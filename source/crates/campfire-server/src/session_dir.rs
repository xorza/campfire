use std::path::{Path, PathBuf};

use campfire_protocol::{DurableError, DurableFile, Journal, JournalError, SessionPrivate};

/// A session's directory under the server's data directory, `sessions/<session id>`: its
/// `private` file, written once before the first offer, and its `journal`. Each is synced into
/// its parent as it is made, so a crash loses neither.
#[derive(Debug)]
pub(crate) struct SessionDir {
    path: PathBuf,
}

impl SessionDir {
    /// Makes the directory of the session whose terms `private` holds, under `data`, and writes
    /// `private` into it.
    pub(crate) fn create(
        data: &Path,
        private: &SessionPrivate,
    ) -> Result<SessionDir, DurableError> {
        let sessions = data.join("sessions");
        DurableFile::create_dir(&sessions)?;
        let path = sessions.join(private.terms.session_id().to_string());
        DurableFile::create_dir(&path)?;
        DurableFile::write(&path.join("private"), &private.encode())?;
        Ok(SessionDir { path })
    }

    /// The session's new journal.
    pub(crate) fn start_journal(&self) -> Result<Journal, JournalError> {
        Journal::create(&self.path.join("journal"))
    }
}
