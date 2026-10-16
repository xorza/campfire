use std::io;
use std::str::Utf8Error;

use campfire_bots::OrderScriptError;
use campfire_codec::JsonError;
use campfire_protocol::LogError;
use campfire_store::{DurableError, PathError, ReadError};
use thiserror::Error;

use crate::process::Process;

/// Why the check could not run to a verdict.
#[derive(Debug, Error)]
pub(crate) enum CheckError {
    /// The path of the check's own executable, beside which it finds the processes, could not
    /// be read.
    #[error("the check's own executable could not be found")]
    OwnExecutable(#[source] io::Error),
    /// The working directory, against which a relative path resolves, could not be read.
    #[error("the working directory could not be read")]
    WorkingDir(#[source] io::Error),
    #[error("a file of the run does not read")]
    Read(#[source] PathError<ReadError>),
    #[error("a file of the run is not written")]
    Write(#[source] PathError<DurableError>),
    /// The file a process writes its text log to is not made.
    #[error("a process's text log is not made")]
    TextLog(#[source] PathError<io::Error>),
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
        error: JsonError,
    },
    #[error("a bot's script")]
    Script(#[source] OrderScriptError),
    /// The session log the server published does not decode.
    #[error("the published session log")]
    SessionLog(#[source] LogError),
}
