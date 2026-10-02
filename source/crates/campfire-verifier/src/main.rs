//! Replays a session log file with the packages under a directory, and logs the state hash after
//! its last tick, in hex. It logs as `campfire_log::Logging` says, `info` by default.

use std::env;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use campfire_log::{LogEvent, Logging};
use campfire_package::PackageStore;
use campfire_protocol::SessionLog;
use campfire_sim::StateHash;
use campfire_verifier::{Replay, Verified};
use tracing::{error, warn};

fn main() -> ExitCode {
    Logging {
        terminal: "info",
        file: "info,campfire_runner=debug,campfire_script=debug",
    }
    .start();
    let mut args = env::args_os().skip(1);
    let (Some(packages), Some(path), None) = (args.next(), args.next(), args.next()) else {
        error!("usage: campfire-verifier <packages directory> <session log file>");
        return ExitCode::from(2);
    };
    let path = Path::new(&path);
    match verify(Path::new(&packages), path) {
        Ok(hash) => {
            Verified {
                file: path.to_owned(),
                hash,
            }
            .log();
            ExitCode::SUCCESS
        }
        Err(error) => {
            error!(file = %path.display(), %error, "the log does not verify");
            ExitCode::FAILURE
        }
    }
}

/// The state hash after the last tick of the log file at `path`, replayed with the packages under
/// `packages`.
fn verify(packages: &Path, path: &Path) -> Result<StateHash, Box<dyn Error>> {
    let store = PackageStore::scan(packages)?;
    for failure in store.failures() {
        warn!(dir = %failure.dir.display(), error = %failure.error, "a package does not read");
    }
    let mut replay = Replay::new(SessionLog::decode(&fs::read(path)?)?, &store)?;
    while replay.run_tick() {}
    Ok(replay.runner().state_hash())
}
