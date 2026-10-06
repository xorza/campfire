use std::process::ExitStatus;

use derive_more::Display;

/// How a process ended.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProcessOutcome {
    #[display("succeeded")]
    Succeeded,
    /// It exited with a failure, with its code when it has one.
    #[display("{}", failed(*code))]
    Failed { code: Option<i32> },
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
            ProcessOutcome::Failed {
                code: status.code(),
            }
        }
    }
}

/// How a process that failed ended: with its exit code, or by a signal when it has none.
fn failed(code: Option<i32>) -> String {
    match code {
        Some(code) => format!("exited with {code}"),
        None => "was killed by a signal".to_owned(),
    }
}
