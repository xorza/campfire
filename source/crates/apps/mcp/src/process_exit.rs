use std::fmt::{self, Display, Formatter};
use std::process::ExitStatus;

/// How a game's or a build's process ended, worded alike on every OS: with its exit code; with
/// none, as a signal ends a process on Unix; with a status that did not read; or not yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProcessExit {
    Code(i32),
    NoCode,
    Unread,
    Running,
}

impl From<ExitStatus> for ProcessExit {
    fn from(status: ExitStatus) -> Self {
        status.code().map_or(Self::NoCode, Self::Code)
    }
}

impl Display for ProcessExit {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Code(code) => write!(formatter, "exit code {code}"),
            Self::NoCode => formatter.write_str("an end of no exit code"),
            Self::Unread => formatter.write_str("an exit status that does not read"),
            Self::Running => formatter.write_str("an end not yet read"),
        }
    }
}
