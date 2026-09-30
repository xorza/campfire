use std::fmt;

/// A process the check runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Process {
    Server,
    /// The bot that plays the script of this index.
    Bot(usize),
    Verifier,
}

impl Process {
    /// The stem of the files its logs go to.
    pub(crate) fn file_stem(self) -> String {
        match self {
            Process::Server => "server".to_owned(),
            Process::Bot(index) => format!("bot-{index}"),
            Process::Verifier => "verifier".to_owned(),
        }
    }
}

impl fmt::Display for Process {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Process::Server => f.write_str("the server"),
            Process::Bot(index) => write!(f, "bot {index}"),
            Process::Verifier => f.write_str("the verifier"),
        }
    }
}
