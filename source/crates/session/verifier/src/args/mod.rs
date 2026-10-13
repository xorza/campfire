use std::path::PathBuf;

use clap::Parser;

/// Replays a session log with the packages its terms name, and logs the state hash after its
/// last tick.
#[derive(Debug, Parser)]
#[command(version)]
pub(crate) struct Args {
    /// The directory of the packages, which holds those the log's terms name
    pub(crate) packages: PathBuf,
    /// The session log file
    pub(crate) log: PathBuf,
    /// The directory of the checkpoints' snapshots, each named by its fingerprint in hex, which
    /// the verifier also checks
    pub(crate) snapshots: Option<PathBuf>,
}

#[cfg(test)]
mod tests;
