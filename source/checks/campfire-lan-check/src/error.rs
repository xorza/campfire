use std::io;
use std::path::PathBuf;

use campfire_net::OrderScriptError;
use campfire_protocol::LogError;
use campfire_store::DataDirError;
use thiserror::Error;

use crate::process::Process;
use crate::target_name::TargetName;

/// Why the check could not run to a verdict.
#[derive(Debug, Error)]
pub(crate) enum CheckError {
    /// It runs only under `cargo run`, which names the cargo that builds the processes.
    #[error("run the check with `cargo run -p campfire-lan-check`")]
    NotUnderCargo,
    #[error("cargo did not start: {0}")]
    CargoStart(io::Error),
    /// Cargo did not build the processes.
    #[error("cargo did not build the processes")]
    Build,
    /// Cargo built no executable of this target.
    #[error("cargo built no executable of {0}")]
    NoExecutable(TargetName),
    /// A file of the run could not be read or written.
    #[error("{}: {error}", .path.display())]
    File { path: PathBuf, error: io::Error },
    #[error("{process} did not start: {error}")]
    Start { process: Process, error: io::Error },
    #[error("could not wait for {process}: {error}")]
    Wait { process: Process, error: io::Error },
    /// A line of a process's log is not an event the check can read.
    #[error("line {line} of the log of {process}: {error}")]
    Event {
        process: Process,
        line: usize,
        error: serde_json::Error,
    },
    /// A host's data directory does not open.
    #[error("{}: {error}", .path.display())]
    Data { path: PathBuf, error: DataDirError },
    #[error("a bot's script: {0}")]
    Script(OrderScriptError),
    /// The session log the server published does not decode.
    #[error("the published session log: {0}")]
    SessionLog(LogError),
}
