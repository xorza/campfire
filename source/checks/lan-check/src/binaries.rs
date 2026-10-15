use std::env;
use std::env::consts::EXE_SUFFIX;
use std::path::PathBuf;

use crate::error::CheckError;

/// The executables the check runs: those beside its own, which `./drive.py` builds with it in the
/// tests' build, so the workspace compiles once.
#[derive(Debug)]
pub(crate) struct Binaries {
    pub(crate) server: PathBuf,
    pub(crate) client: PathBuf,
    pub(crate) verifier: PathBuf,
}

impl Binaries {
    /// The server's, client's and verifier's binaries, each its package's name, which the
    /// driver's `LAN_BINARIES` builds with the check: one it left out would be an old build.
    const NAMES: [&str; 3] = ["campfire-server", "campfire-client", "campfire-verifier"];

    /// The server, client and verifier in the folder of the check's own executable.
    pub(crate) fn beside_check() -> Result<Binaries, CheckError> {
        let own = env::current_exe().map_err(CheckError::OwnExecutable)?;
        let dir = own.parent().expect("an executable is in a folder");
        let [server, client, verifier] =
            Binaries::NAMES.map(|name| dir.join(format!("{name}{EXE_SUFFIX}")));
        Ok(Binaries {
            server,
            client,
            verifier,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_driver_builds_the_check_and_each_binary_it_runs() {
        let driver = include_str!("../../../drive.py");
        let list = driver
            .split_once("LAN_BINARIES = (")
            .and_then(|(_, rest)| rest.split_once(')'))
            .map(|(list, _)| list)
            .expect("the driver lists the LAN binaries");
        let mut built: Vec<&str> = list
            .split(',')
            .map(|name| name.trim().trim_matches('"'))
            .filter(|name| !name.is_empty())
            .collect();
        built.sort_unstable();
        let mut run = vec!["campfire-lan-check"];
        run.extend(Binaries::NAMES);
        run.sort_unstable();
        assert_eq!(built, run);
    }
}
