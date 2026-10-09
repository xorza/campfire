use std::io;
use std::str::Utf8Error;

use campfire_net::OrderScriptError;
use campfire_protocol::LogError;
use campfire_store::{DurableCreateError, DurableError, PathError, ReadError};
use thiserror::Error;

use crate::process::Process;
use crate::target_name::TargetName;

/// Why the check could not run to a verdict.
#[derive(Debug, Error)]
pub(crate) enum CheckError {
    /// It runs only under `cargo run`, which names the cargo that builds the processes, unless
    /// it verifies with a verifier it is given.
    #[error(
        "run the check with `cargo run -p campfire-lan-check`, or give `verify` a `--verifier`"
    )]
    NotUnderCargo,
    #[error("cargo did not start")]
    CargoStart(#[source] io::Error),
    /// The working directory, against which a relative path resolves, could not be read.
    #[error("the working directory could not be read")]
    WorkingDir(#[source] io::Error),
    /// Cargo did not build the processes.
    #[error("cargo did not build the processes")]
    Build,
    /// Cargo built no executable of this target.
    #[error("cargo built no executable of {0}")]
    NoExecutable(TargetName),
    #[error("a file of the run does not read")]
    Read(#[source] PathError<ReadError>),
    #[error("a file of the run is not written")]
    Write(#[source] PathError<DurableError>),
    /// The run's directory is not made, or another run's holds its name.
    #[error("the run's directory is not made")]
    RunDir(#[source] PathError<DurableCreateError>),
    /// The file a process writes its text log to is not made.
    #[error("a process's text log is not made")]
    TextLog(#[source] PathError<io::Error>),
    /// No UDP port on `127.0.0.1` was free for the server to listen on.
    #[error("no free port on the loopback address")]
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
    /// A process's log, its whole lines, is not UTF-8.
    #[error("the log of {process} is not UTF-8")]
    NotText {
        process: Process,
        #[source]
        error: Utf8Error,
    },
    /// A line of a process's log is not an event the check can read.
    #[error("line {line} of the log of {process}")]
    Event {
        process: Process,
        line: usize,
        #[source]
        error: serde_json::Error,
    },
    #[error("a bot's script")]
    Script(#[source] OrderScriptError),
    /// The session log the server published does not decode.
    #[error("the published session log")]
    SessionLog(#[source] LogError),
}
