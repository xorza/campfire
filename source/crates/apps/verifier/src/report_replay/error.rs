use std::io;

use campfire_capabilities::ProbeRefusal;
use campfire_common::Tick;
use campfire_host::{ReportLogError, ReportReadError};
use campfire_package::LoadError;
use campfire_runner::{ReplayError, StartError};
use campfire_store::{DurableDirError, DurableError, PathError};
use thiserror::Error;

/// Why a report does not replay, by the step that failed.
#[derive(Debug, Error)]
pub enum ReportReplayError {
    #[error("the report's facts do not read")]
    Facts(#[source] ReportReadError),
    #[error("the report's log does not read")]
    Log(#[source] ReportLogError),
    /// The mode does not load from the directory the facts name.
    #[error("the report's mode does not load")]
    Packages(#[source] LoadError),
    /// The log's match does not start on the mode.
    #[error("the replay does not start")]
    Start(#[source] StartError),
    /// The facts name a tick the log has not run, which a server writes only once its log ran
    /// the tick.
    #[error("the report's tick {tick} is not in its log, which ends before tick {end}")]
    PastLog { tick: Tick, end: Tick },
    #[error("the probe does not probe the match")]
    Probe(#[source] ProbeRefusal),
    /// The replay does not reach what the log records.
    #[error("the log does not replay to what it records")]
    Replay(#[source] ReplayError),
    #[error("the probe's directory is not made")]
    ProbeDir(#[source] PathError<DurableDirError>),
    #[error("the probe's file does not write")]
    ProbeFile(#[source] PathError<DurableError>),
    #[error("the table does not print")]
    Print(#[source] io::Error),
}
