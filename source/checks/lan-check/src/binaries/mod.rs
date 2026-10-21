use std::env;
use std::env::consts::EXE_SUFFIX;
use std::path::PathBuf;

use crate::error::CheckError;

/// The executables the check runs: those beside its own, which `cargo drive run lan` builds with it
/// in one build.
#[derive(Debug)]
pub(crate) struct Binaries {
    pub(crate) server: PathBuf,
    pub(crate) client: PathBuf,
    pub(crate) verifier: PathBuf,
}

impl Binaries {
    /// The server's, client's and verifier's binaries, each its package's name, which the `lan`
    /// command of the workspace's drive table builds with the check: one it left out would be an
    /// old build.
    const NAMES: [&str; 3] = ["campfire-server", "campfire-client", "campfire-verifier"];

    /// The server, client and verifier in the directory of the check's own executable.
    pub(crate) fn beside_check() -> Result<Self, CheckError> {
        let own = env::current_exe().map_err(CheckError::OwnExecutable)?;
        let dir = own.parent().expect("an executable is in a directory");
        let [server, client, verifier] =
            Self::NAMES.map(|name| dir.join(format!("{name}{EXE_SUFFIX}")));
        Ok(Self {
            server,
            client,
            verifier,
        })
    }
}

#[cfg(test)]
mod tests;
