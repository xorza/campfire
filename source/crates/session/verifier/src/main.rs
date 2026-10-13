//! Replays a session log file with the packages under a directory, and logs the state hash after
//! its last tick, in hex; with a directory of snapshots, each named by its fingerprint in hex, it
//! also checks the snapshot of each checkpoint. It logs as `campfire_log::Logging` says, `info`
//! by default.

use std::env;
use std::process::ExitCode;

use campfire_common::ExitStatus;
use campfire_log::{ErrorReport, LogEvent, Logging};
use campfire_verifier::{Verification, Verified};
use tracing::error;

use crate::args::Args;

mod args;

fn main() -> ExitCode {
    let _log = Logging {
        terminal: "info",
        file: "info,campfire_runner=debug,campfire_script=debug",
    }
    .start();
    let args: Args = match Logging::command_line(env::args_os()) {
        Ok(args) => args,
        Err(status) => return ExitCode::from(status),
    };
    match Verification::file(&args.packages, &args.log, args.snapshots.as_deref()) {
        Ok(hash) => {
            Verified {
                file: args.log,
                hash,
            }
            .log();
            ExitCode::from(ExitStatus::Success)
        }
        Err(error) => {
            error!(file = %args.log.display(), error = %ErrorReport::of(&error), "the log does not verify");
            ExitCode::from(ExitStatus::Failure)
        }
    }
}
