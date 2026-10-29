//! Verifies a session log file with the packages under a directory, and logs the state hash after
//! its last tick, in hex; with a directory of snapshots, each named by its fingerprint in hex, it
//! also checks the snapshot of each checkpoint. Or prints a session log as text. Or replays a
//! player's bug report to the tick its client drew, and prints a table of the units there. It logs
//! as `campfire_logging::Logging` says, `info` by default.

use std::env;
use std::io;
use std::process::ExitCode;

use campfire_common::ErrorReport;
use campfire_common::ExitStatus;
use campfire_log::LogEvent;
use campfire_logging::Logging;
use campfire_verifier::{ReportProbe, Verified, replay_report, show_file, verify_file};
use tracing::error;

use crate::command_line::{CommandLine, VerifierCommand};

mod command_line;

fn main() -> ExitCode {
    let _log = Logging {
        terminal: "info",
        file: "info,campfire_runner=debug,campfire_script=debug",
    }
    .start();
    let line: CommandLine = match Logging::command_line(env::args_os()) {
        Ok(line) => line,
        Err(status) => return ExitCode::from(status),
    };
    match line.command {
        VerifierCommand::Verify {
            packages,
            log,
            snapshots,
        } => match verify_file(&packages, &log, snapshots.as_deref()) {
            Ok(hash) => {
                Verified { file: log, hash }.log();
                ExitCode::from(ExitStatus::Success)
            }
            Err(error) => {
                error!(file = %log.display(), error = %ErrorReport::of(&error), "the log does not verify");
                ExitCode::from(ExitStatus::Failure)
            }
        },
        VerifierCommand::Show { log } => match show_file(&log, &mut io::stdout().lock()) {
            Ok(()) => ExitCode::from(ExitStatus::Success),
            Err(error) => {
                error!(file = %log.display(), error = %ErrorReport::of(&error), "the log does not show");
                ExitCode::from(ExitStatus::Failure)
            }
        },
        VerifierCommand::Report {
            report,
            probe,
            probe_dir,
        } => {
            let probe = probe.map(|spec| ReportProbe {
                spec,
                dir: probe_dir,
            });
            match replay_report(&report, probe.as_ref(), io::stdout().lock()) {
                Ok(()) => ExitCode::from(ExitStatus::Success),
                Err(error) => {
                    error!(report = %report.display(), error = %ErrorReport::of(&error), "the report does not replay");
                    ExitCode::from(ExitStatus::Failure)
                }
            }
        }
    }
}
