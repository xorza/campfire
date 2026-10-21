use std::io;
use std::net::{SocketAddr, SocketAddrV4, UdpSocket};

use socket2::{Domain, Protocol, Socket, Type};

use crate::platform::os::Native;

/// A UDP socket on a port that other sockets of this host share, as each SSDP listener of a host
/// binds port 1900: each takes the multicast datagrams sent to the port. On Unix,
/// `SO_REUSEADDR` and `SO_REUSEPORT`, as BSD and macOS bind a port twice only with the second; on
/// Windows, `SO_REUSEADDR`. A socket bound without sharing keeps its port its own on every OS.
#[derive(Debug)]
pub struct SharedPort {
    socket: UdpSocket,
}

impl SharedPort {
    pub fn bind(address: SocketAddrV4) -> io::Result<Self> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        Native::share_port(&socket)?;
        socket.bind(&SocketAddr::V4(address).into())?;
        Ok(Self {
            socket: socket.into(),
        })
    }

    pub const fn socket(&self) -> &UdpSocket {
        &self.socket
    }
}

#[cfg(test)]
mod tests;
