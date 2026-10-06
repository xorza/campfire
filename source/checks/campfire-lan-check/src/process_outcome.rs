use std::fmt;
use std::process::ExitStatus;

/// How a process ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProcessOutcome {
    Succeeded,
    /// It exited with a failure, with its code when it has one.
    Failed {
        code: Option<i32>,
    },
    /// It still ran at the deadline, and the check stopped it.
    Overran,
    /// The check stopped it mid-match, as the run asks.
    Stopped,
    /// The check did not start it: the server never listened.
    NotStarted,
}

impl ProcessOutcome {
    pub(crate) fn of(status: ExitStatus) -> ProcessOutcome {
        if status.success() {
            ProcessOutcome::Succeeded
        } else {
            ProcessOutcome::Failed {
                code: status.code(),
            }
        }
    }
}

/// How a process ended, as the end of a sentence about it.
impl fmt::Display for ProcessOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProcessOutcome::Succeeded => f.write_str("succeeded"),
            ProcessOutcome::Failed { code: Some(code) } => write!(f, "exited with {code}"),
            ProcessOutcome::Failed { code: None } => f.write_str("was killed by a signal"),
            ProcessOutcome::Overran => f.write_str("still ran at the deadline"),
            ProcessOutcome::Stopped => f.write_str("was stopped mid-match"),
            ProcessOutcome::NotStarted => f.write_str("did not start"),
        }
    }
}
