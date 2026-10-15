use std::io;

use thiserror::Error;

/// Why a stream writer stopped, by its step. Neither is retried: the bytes of a failed write may
/// be half written, and the stream's order would break.
#[derive(Debug, Error)]
pub enum StreamError {
    #[error("could not write")]
    Write(#[source] io::Error),
    #[error("could not flush what was written")]
    Flush(#[source] io::Error),
}
