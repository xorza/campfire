use std::io;

use campfire_store::DurableError;
use thiserror::Error;
use wtransport::tls::error::InvalidCertificate;

/// Why the server's TLS identity did not open.
#[derive(Debug, Error)]
pub(crate) enum TlsError {
    #[error("could not read the TLS identity")]
    Read(#[source] io::Error),
    /// The file ends inside a field, or holds no key.
    #[error("the TLS identity's file is cut short")]
    Truncated,
    #[error("the TLS certificate does not read")]
    Certificate(#[source] InvalidCertificate),
    #[error("could not write the TLS identity")]
    Write(#[source] DurableError),
}
