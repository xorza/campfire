use campfire_log::{ErrorReport, LogEvent};
use campfire_store::{AppendError, DurableError, PathError};

use crate::events::checkpoint_failed::CheckpointFailed;
use crate::events::journal_failed::JournalFailed;
use crate::events::receipt_unsaved::ReceiptUnsaved;

/// A worker's failure, typed by the step that failed, with its source.
#[derive(Debug)]
pub(crate) enum Fault {
    /// The session's journal stopped at a failed write or sync.
    Journal(AppendError),
    /// A checkpoint's snapshot was not written.
    Snapshot(PathError<DurableError>),
    /// A client's newest receipt was not written.
    Receipt(PathError<DurableError>),
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
    /// The table of policies: a journal or a snapshot that fails ends the server, as the session
    /// it holds keeps no record past it; a receipt that is not written is logged, as the client
    /// keeps it in memory and the next replaces it.
    pub(crate) const fn policy(&self) -> FaultPolicy {
        match self {
            Self::Journal(_) | Self::Snapshot(_) => FaultPolicy::EndServer,
            Self::Receipt(_) => FaultPolicy::Log,
        }
    }

    /// Logs the fault's event.
    pub(crate) fn log(&self) {
        match self {
            Self::Journal(error) => JournalFailed {
                error: ErrorReport::of(error).to_string(),
            }
            .log(),
            Self::Snapshot(error) => CheckpointFailed {
                error: ErrorReport::of(error).to_string(),
            }
            .log(),
            Self::Receipt(error) => ReceiptUnsaved {
                error: ErrorReport::of(error).to_string(),
            }
            .log(),
        }
    }
}
