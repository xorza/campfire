use std::io;
use std::net::SocketAddr;

/// An endpoint's socket did not open on `address`: the port is in use, or the OS refused it.
#[derive(Debug, thiserror::Error)]
#[error("a UDP socket does not open on {address}")]
pub struct BindError {
    pub address: SocketAddr,
    #[source]
    pub error: io::Error,
}
