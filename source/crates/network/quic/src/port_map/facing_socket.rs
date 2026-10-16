use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};

/// A UDP socket connected to a peer on IPv4, and this host's address that faces it, which the OS
/// picks by its routes.
#[derive(Debug)]
pub(crate) struct FacingSocket {
    pub(crate) socket: UdpSocket,
    pub(crate) host: Ipv4Addr,
}

impl FacingSocket {
    pub(crate) fn connect(peer: SocketAddrV4) -> io::Result<Self> {
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
        socket.connect(peer)?;
        let SocketAddr::V4(local) = socket.local_addr()? else {
            unreachable!("a socket bound to an IPv4 address keeps one");
        };
        Ok(Self {
            socket,
            host: *local.ip(),
        })
    }
}
