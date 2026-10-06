//! Replays a session log file with the packages under a directory, and logs the state hash after
//! its last tick, in hex; with a directory of snapshots, each named by its fingerprint in hex, it
//! also checks the snapshot of each checkpoint. It logs as `campfire_log::Logging` says, `info`
//! by default.

use std::env;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use campfire_common::{ExitStatus, StateHash};
use campfire_log::{ErrorReport, LogEvent, Logging};
use campfire_package::PackageStore;
use campfire_protocol::SessionLog;
use campfire_verifier::{Replay, Verified};
use tracing::{error, warn};

use crate::args::Args;

mod args;

fn main() -> ExitCode {
    Logging {
        terminal: "info",
        file: "info,campfire_runner=debug,campfire_script=debug",
    }
    .start();
    let args: Args = match Logging::command_line(env::args_os()) {
        Ok(args) => args,
        Err(status) => return ExitCode::from(status),
    };
    match verify(&args.packages, &args.log, args.snapshots.as_deref()) {
        Ok(hash) => {
            Verified {
                file: args.log,
                hash,
            }
            .log();
            ExitCode::from(ExitStatus::Success)
        }
        Err(error) => {
            error!(file = %args.log.display(), error = %ErrorReport::of(&*error), "the log does not verify");
            ExitCode::from(ExitStatus::Failure)
        }
    }
}

/// The state hash after the last tick of the log file at `path`, replayed with the packages under
/// `packages`, once each checkpoint's snapshot under `snapshots`, when given, checks.
fn verify(
    packages: &Path,
    path: &Path,
    snapshots: Option<&Path>,
) -> Result<StateHash, Box<dyn Error>> {
    let store = PackageStore::scan(packages)?;
    for failure in store.failures() {
        warn!(dir = %failure.dir.display(), error = %ErrorReport::of(&failure.error), "a package does not read");
    }
    let mut replay = Replay::new(SessionLog::decode(&fs::read(path)?)?, &store)?;
    if let Some(dir) = snapshots {
        for record in replay.runner().log().checkpoints() {
            let snapshot = fs::read(dir.join(record.snapshot.to_string()))?;
            replay.check_snapshot(record, &snapshot)?;
        }
    }
    while replay.run_tick()? {}
    Ok(replay.runner().state_hash())
}
