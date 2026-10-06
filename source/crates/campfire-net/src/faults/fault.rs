use campfire_log::LogEvent;
use campfire_store::{AppendError, DurableError};

use crate::events::checkpoint_failed::CheckpointFailed;
use crate::events::journal_failed::JournalFailed;
use crate::events::receipt_unsaved::ReceiptUnsaved;

/// A worker's failure, typed by the step that failed, with its source.
#[derive(Debug)]
pub(crate) enum Fault {
    /// The session's journal stopped at a failed write or sync.
    Journal(AppendError),
    /// A checkpoint's snapshot was not written.
    Snapshot(DurableError),
    /// A client's newest receipt was not written.
    Receipt(DurableError),
}

/// Where a fault comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FaultSource {
    Journal,
    Snapshot,
    Receipt,
}

/// What a fault does, beyond its log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FaultPolicy {
    /// The server exits with `ExitStatus::Storage`, as it keeps no record past the fault;
    /// its host's supervisor starts it again, which restores the session.
    EndServer,
    /// The fault is logged, and play goes on.
    Log,
}

impl Fault {
    pub(crate) const fn source(&self) -> FaultSource {
        match self {
            Fault::Journal(_) => FaultSource::Journal,
            Fault::Snapshot(_) => FaultSource::Snapshot,
            Fault::Receipt(_) => FaultSource::Receipt,
        }
    }

    /// Logs the fault's event.
    pub(crate) fn log(&self) {
        match self {
            Fault::Journal(error) => JournalFailed {
                error: error.to_string(),
            }
            .log(),
            Fault::Snapshot(error) => CheckpointFailed {
                error: error.to_string(),
            }
            .log(),
            Fault::Receipt(error) => ReceiptUnsaved {
                error: error.to_string(),
            }
            .log(),
        }
    }
}

impl FaultSource {
    /// The table of policies: a journal or a snapshot that fails ends the server, as the session
    /// it holds keeps no record past it; a receipt that is not written is logged, as the client
    /// keeps it in memory and the next replaces it.
    pub(crate) const fn policy(self) -> FaultPolicy {
        match self {
            FaultSource::Journal | FaultSource::Snapshot => FaultPolicy::EndServer,
            FaultSource::Receipt => FaultPolicy::Log,
        }
    }
}
