use std::path::{Path, PathBuf};

use derive_more::Display;

/// A process the check runs.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Process {
    #[display("the server")]
    Server,
    /// The server started again on its data directory, after the check stopped it.
    #[display("the server, started again")]
    ServerAgain,
    /// The bot that plays the script of this index.
    #[display("bot {_0}")]
    Bot(usize),
    /// The bot of this index, started again with its key file after the check stopped it.
    #[display("bot {_0}, started again")]
    Rejoined(usize),
    /// A bot that pins a certificate no server has, so it never links.
    #[display("the impostor bot")]
    Impostor,
    #[display("the verifier")]
    Verifier,
    /// A client bot that plays alone on a local server, against a server bot.
    #[display("the local client")]
    Local,
    /// The verifier of the local server's session log.
    #[display("the verifier of the local session")]
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
