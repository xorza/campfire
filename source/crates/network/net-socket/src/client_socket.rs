use core::net::{Ipv4Addr, Ipv6Addr, SocketAddr};

use campfire_net_core::Random;
use campfire_net_packets::{DatagramIo, EndpointPieces};
use campfire_net_udp::UdpClient;

use crate::bind_error::BindError;
use crate::udp_io::UdpIo;

/// A client's socket, bound to a port of the OS's on the unspecified address of its server's
/// family, and the client of the UDP protocol it carries.
#[derive(Debug)]
pub struct ClientSocket {
    io: UdpIo,
    server: SocketAddr,
    client: UdpClient,
}

impl ClientSocket {
    /// A client's socket toward `server`, which carries `client`.
    pub fn bind(server: SocketAddr, client: UdpClient) -> Result<Self, BindError> {
        let unspecified = if server.is_ipv4() {
            Ipv4Addr::UNSPECIFIED.into()
        } else {
            Ipv6Addr::UNSPECIFIED.into()
        };
        Ok(Self {
            io: UdpIo::bind(SocketAddr::new(unspecified, 0))?,
            server,
            client,
        })
    }

    pub fn local_address(&self) -> SocketAddr {
        self.io.local_address()
    }

    /// The parts of the client's endpoint, which connects at its `LinkStart`.
    pub fn endpoint(self) -> EndpointPieces<UdpIo> {
        self.client.endpoint(self.io, self.server, Random::os())
    }
}
