use std::fmt;
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
    /// The check stopped it mid-match, as the run asks.
    Stopped,
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

/// How a process ended, as the end of a sentence about it.
impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Outcome::Succeeded => f.write_str("succeeded"),
            Outcome::Failed { code: Some(code) } => write!(f, "exited with {code}"),
            Outcome::Failed { code: None } => f.write_str("was killed by a signal"),
            Outcome::Overran => f.write_str("still ran at the deadline"),
            Outcome::Stopped => f.write_str("was stopped mid-match"),
            Outcome::NotStarted => f.write_str("did not start"),
        }
    }
}
