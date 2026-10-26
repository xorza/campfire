use std::path::PathBuf;

use campfire_common::Tick;
use campfire_protocol::SessionId;
use campfire_store::{DurableDir, DurableDirCreateError};

use crate::client::report_dir::ReportDir;
use crate::error::ReportWriteError;
use crate::messages::journal_command::ReportKey;

/// The paths of a client's data directory, the one place they are named: the newest receipt of
/// each session, `receipts/<session id>.receipt`; its local server's data directory, `server`, a
/// data directory of its own with its own lock; and each report's folder under `reports`. Made
/// from the directory's path alone, so a tool reads a finished run's directory it does not hold
/// by it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientLayout {
    root: PathBuf,
}

/// A report's folder the client made, and the key the server writes its log into it by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewReport {
    pub dir: ReportDir,
    pub key: ReportKey,
}

impl ClientLayout {
    pub const fn at(root: PathBuf) -> Self {
        Self { root }
    }

    pub(crate) fn receipts_dir(&self) -> PathBuf {
        self.root.join("receipts")
    }

    /// The newest receipt of the session of `session_id`.
    pub(crate) fn receipt_file(&self, session_id: SessionId) -> PathBuf {
        self.receipts_dir().join(format!("{session_id}.receipt"))
    }

    /// The folder of the report `key` of the session of `session_id`.
    pub fn report_dir(&self, session_id: SessionId, key: ReportKey) -> ReportDir {
        let ReportKey { tick, take } = key;
        let name = match take {
            0 => format!("{session_id}-{tick}"),
            _ => format!("{session_id}-{tick}-{take}"),
        };
        ReportDir::at(self.reports_dir().join(name))
    }

    /// Makes the folder of a new report of the session of `session_id` at `tick`: the first take
    /// of the tick whose folder no earlier report made.
    pub fn create_report_dir(
        &self,
        session_id: SessionId,
        tick: Tick,
    ) -> Result<NewReport, ReportWriteError> {
        DurableDir::create_all(&self.reports_dir())?;
        for take in 0..=u32::MAX {
            let key = ReportKey { tick, take };
            let dir = self.report_dir(session_id, key);
            match DurableDir::create_new(dir.path()) {
                Ok(()) => return Ok(NewReport { dir, key }),
                Err(made) if matches!(made.error, DurableDirCreateError::Exists) => {}
                Err(made) => return Err(made.into()),
            }
        }
        unreachable!("a tick holds fewer reports than a u32 counts")
    }

    fn reports_dir(&self) -> PathBuf {
        self.root.join("reports")
    }

    /// The data directory of the client's local server.
    pub fn local_server_dir(&self) -> PathBuf {
        self.root.join("server")
    }
}
