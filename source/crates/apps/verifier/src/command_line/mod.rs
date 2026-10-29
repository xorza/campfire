use std::path::PathBuf;

use campfire_capabilities::ProbeSpec;
use clap::{Parser, Subcommand};

/// Verifies a session log, prints one as text, or replays a player's bug report to the tick its
/// client drew.
#[derive(Debug, Parser)]
#[command(version)]
pub(crate) struct CommandLine {
    #[command(subcommand)]
    pub(crate) command: VerifierCommand,
}

/// What the verifier does.
#[derive(Debug, Subcommand)]
pub(crate) enum VerifierCommand {
    /// Replays a session log with the packages its terms name, and logs the state hash after its
    /// last tick
    Verify {
        /// The directory of the packages, which holds those the log's terms name
        packages: PathBuf,
        /// The session log file
        log: PathBuf,
        /// The directory of the checkpoints' snapshots, each named by its fingerprint in hex,
        /// which the verifier also checks
        snapshots: Option<PathBuf>,
    },
    /// Prints a session log as text: its terms, each segment and its checkpoint, each tick's
    /// entries, the seed reveal and the result
    Show {
        /// The session log file
        log: PathBuf,
    },
    /// Replays a report's log to the tick its client drew, with the mode its facts name, and
    /// prints a table of the units there
    Report {
        /// The report's folder, as the client wrote it in its data directory's `reports`
        report: PathBuf,
        /// A probe of the units before the report's tick, `<period_ms>:<units>`, the units
        /// `all`, `player=<n>` from 1, `team=<n>`, `type=<name>`, `ids=<id>,<id>` or
        /// `state=<state>`; it writes
        /// `<session>-<tick>.probe` in the probes' directory, and the table at the tick takes its
        /// units
        #[arg(long)]
        probe: Option<ProbeSpec>,
        /// The directory the probe writes to
        #[arg(long, default_value = "target/report")]
        probe_dir: PathBuf,
    },
}

#[cfg(test)]
mod tests;
