use std::fmt;
use std::path::{Path, PathBuf};

/// A process the check runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Process {
    Server,
    /// The server started again on its data directory, after the check stopped it.
    ServerAgain,
    /// The bot that plays the script of this index.
    Bot(usize),
    /// The bot of this index, started again with its key file after the check stopped it.
    Rejoined(usize),
    /// A bot that pins a certificate no server has, so it never links.
    Impostor,
    Verifier,
    /// A client bot that plays alone on a local server, against a server bot.
    Local,
    /// The verifier of the local server's session log.
    LocalVerifier,
}

impl Process {
    /// The file in `dir` that it logs JSON to.
    pub(crate) fn log_path(self, dir: &Path) -> PathBuf {
        dir.join(format!("{}.jsonl", self.file_stem()))
    }

    /// The stem of the files its logs go to.
    pub(crate) fn file_stem(self) -> String {
        match self {
            Process::Server => "server".to_owned(),
            Process::ServerAgain => "server-again".to_owned(),
            Process::Bot(index) => format!("bot-{index}"),
            Process::Rejoined(index) => format!("bot-{index}-again"),
            Process::Impostor => "impostor".to_owned(),
            Process::Verifier => "verifier".to_owned(),
            Process::Local => "local".to_owned(),
            Process::LocalVerifier => "local-verifier".to_owned(),
        }
    }
}

impl fmt::Display for Process {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Process::Server => f.write_str("the server"),
            Process::ServerAgain => f.write_str("the server, started again"),
            Process::Bot(index) => write!(f, "bot {index}"),
            Process::Rejoined(index) => write!(f, "bot {index}, started again"),
            Process::Impostor => f.write_str("the impostor bot"),
            Process::Verifier => f.write_str("the verifier"),
            Process::Local => f.write_str("the local client"),
            Process::LocalVerifier => f.write_str("the verifier of the local session"),
        }
    }
}
