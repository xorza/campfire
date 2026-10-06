use std::error::Error;
use std::fmt;
use std::io;
use std::path::PathBuf;

use campfire_net::OrderScriptError;
use campfire_protocol::LogError;
use campfire_store::DataDirError;

use crate::process::Process;
use crate::target_name::TargetName;

/// Why the check could not run to a verdict.
#[derive(Debug)]
pub(crate) enum CheckError {
    /// It runs only under `cargo run`, which names the cargo that builds the processes.
    NotUnderCargo,
    CargoStart(io::Error),
    /// Cargo did not build the processes.
    Build,
    /// Cargo built no executable of this target.
    NoExecutable(TargetName),
    /// A file of the run could not be read or written.
    File {
        path: PathBuf,
        error: io::Error,
    },
    Start {
        process: Process,
        error: io::Error,
    },
    Wait {
        process: Process,
        error: io::Error,
    },
    /// A line of a process's log is not an event the check can read.
    Event {
        process: Process,
        line: usize,
        error: serde_json::Error,
    },
    /// A host's data directory does not open.
    Data {
        path: PathBuf,
        error: DataDirError,
    },
    Script(OrderScriptError),
    /// The session log the server published does not decode.
    SessionLog(LogError),
}

impl fmt::Display for CheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CheckError::NotUnderCargo => {
                f.write_str("run the check with `cargo run -p campfire-lan-check`")
            }
            CheckError::CargoStart(error) => write!(f, "cargo did not start: {error}"),
            CheckError::Build => f.write_str("cargo did not build the processes"),
            CheckError::NoExecutable(target) => write!(f, "cargo built no executable of {target}"),
            CheckError::File { path, error } => write!(f, "{}: {error}", path.display()),
            CheckError::Start { process, error } => write!(f, "{process} did not start: {error}"),
            CheckError::Wait { process, error } => {
                write!(f, "could not wait for {process}: {error}")
            }
            CheckError::Event {
                process,
                line,
                error,
            } => write!(f, "line {line} of the log of {process}: {error}"),
            CheckError::Data { path, error } => write!(f, "{}: {error}", path.display()),
            CheckError::Script(error) => write!(f, "a bot's script: {error}"),
            CheckError::SessionLog(error) => write!(f, "the published session log: {error}"),
        }
    }
}

impl Error for CheckError {}
