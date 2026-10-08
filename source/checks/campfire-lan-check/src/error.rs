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
    #[error("cargo did not start")]
    CargoStart(#[source] io::Error),
    /// Cargo did not build the processes.
    #[error("cargo did not build the processes")]
    Build,
    /// Cargo built no executable of this target.
    #[error("cargo built no executable of {0}")]
    NoExecutable(TargetName),
    /// A file of the run could not be read or written.
    #[error("{} could not be read or written", .path.display())]
    File {
        path: PathBuf,
        #[source]
        error: io::Error,
    },
    /// No UDP port on `127.0.0.1` was free for the server to listen on.
    #[error("no free port on 127.0.0.1")]
    Port(#[source] io::Error),
    #[error("{process} did not start")]
    Start {
        process: Process,
        #[source]
        error: io::Error,
    },
    #[error("could not wait for {process}")]
    Wait {
        process: Process,
        #[source]
        error: io::Error,
    },
    /// A line of a process's log is not an event the check can read.
    #[error("line {line} of the log of {process}")]
    Event {
        process: Process,
        line: usize,
        #[source]
        error: serde_json::Error,
    },
    /// A host's data directory does not open.
    #[error("{} does not open", .path.display())]
    Data {
        path: PathBuf,
        #[source]
        error: DataDirError,
    },
    #[error("a bot's script")]
    Script(#[source] OrderScriptError),
    /// The session log the server published does not decode.
    #[error("the published session log")]
    SessionLog(#[source] LogError),
}
