use core::net::SocketAddr;

use campfire_net_core::Random;
use campfire_net_core::TlsKey;
use campfire_net_packets::{DatagramIo, EndpointPieces};
use campfire_net_udp::{AdmissionSetup, UdpServer};

use crate::bind_error::BindError;
use crate::udp_io::UdpIo;

/// A server's socket, bound, and the server of the UDP protocol it carries: it serves once its
/// admission is known, so no server's endpoint exists without one.
#[derive(Debug)]
pub struct ServerSocket {
    io: UdpIo,
    server: UdpServer,
}

impl ServerSocket {
    /// A server's socket on `address`, which carries `server`. It binds now, before its app runs,
    /// so a port in use is this error, not a server with no link.
    pub fn bind(address: SocketAddr, server: UdpServer) -> Result<Self, BindError> {
        Ok(Self {
            io: UdpIo::bind(address)?,
            server,
        })
    }

    /// The address the socket is bound to, its port the OS's when the address named 0.
    pub fn local_address(&self) -> SocketAddr {
        self.io.local_address()
    }

    /// The raw key the server presents, which its clients pin; `None` for a certificate.
    pub const fn tls_key(&self) -> Option<TlsKey> {
        self.server.tls_key()
    }

    /// The parts of the server's endpoint, which admits each address within `admission`.
    pub fn endpoint(self, admission: AdmissionSetup) -> EndpointPieces<UdpIo> {
        self.server.endpoint(self.io, admission, Random::os())
    }
}

#[cfg(test)]
mod tests;
