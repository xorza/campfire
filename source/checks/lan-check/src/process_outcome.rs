use std::process::ExitStatus;

use derive_more::Display;

/// How a process ended.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProcessOutcome {
    #[display("succeeded")]
    Succeeded,
    /// It ended with a failure, whose status std describes as the OS gave it.
    #[display("ended with a failure, {_0}")]
    Failed(ExitStatus),
    /// It still ran at the deadline, and the check stopped it.
    #[display("still ran at the deadline")]
    Overran,
    /// The check stopped it mid-match, as the run asks.
    #[display("was stopped mid-match")]
    Stopped,
    /// The check did not start it: the server never listened.
    #[display("did not start")]
    NotStarted,
}

impl ProcessOutcome {
    pub(crate) fn of(status: ExitStatus) -> ProcessOutcome {
        if status.success() {
            ProcessOutcome::Succeeded
        } else {
            ProcessOutcome::Failed(status)
        }
    }
}
