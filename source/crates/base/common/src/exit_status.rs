use std::process::ExitCode;

/// How a binary of the engine ends: the exit codes its process gives, which the tools and a
/// host's supervisor read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitStatus {
    Success,
    /// A failure that no other status names.
    Failure,
    /// The command line is not one the binary takes, as `clap` and the shells give it.
    Usage,
    /// A write or a sync of a server's storage failed, so it keeps no record past it: sysexits'
    /// `EX_IOERR`. The host's supervisor starts the server again, which restores the session.
    Storage,
}

impl ExitStatus {
    pub const fn code(self) -> u8 {
        match self {
            ExitStatus::Success => 0,
            ExitStatus::Failure => 1,
            ExitStatus::Usage => 2,
            ExitStatus::Storage => 74,
        }
    }
}

impl From<ExitStatus> for ExitCode {
    fn from(status: ExitStatus) -> ExitCode {
        ExitCode::from(status.code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_status_has_its_code() {
        let all = [
            ExitStatus::Success,
            ExitStatus::Failure,
            ExitStatus::Usage,
            ExitStatus::Storage,
        ];
        assert_eq!(all.map(ExitStatus::code), [0, 1, 2, 74]);
    }
}
