use std::process::ExitStatus;

/// How a process ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    Succeeded,
    /// It exited with a failure, with its code when it has one.
    Failed {
        code: Option<i32>,
    },
    /// It still ran at the deadline, and the check stopped it.
    Overran,
    /// The check did not start it: the server never listened.
    NotStarted,
}

impl Outcome {
    pub(crate) fn of(status: ExitStatus) -> Outcome {
        if status.success() {
            Outcome::Succeeded
        } else {
            Outcome::Failed {
                code: status.code(),
            }
        }
    }
}
