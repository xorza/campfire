use std::io;
use std::net::{SocketAddr, SocketAddrV4, UdpSocket};

use socket2::{Domain, Protocol, Socket, Type};

use crate::platform::os::Os;

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
        Os::share_port(&socket)?;
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
mod tests {
    use std::io::ErrorKind;
    use std::net::Ipv4Addr;

    use super::*;

    #[test]
    fn a_shared_port_binds_twice_and_an_unshared_socket_not_beside_it() {
        let first = SharedPort::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).unwrap();
        let SocketAddr::V4(address) = first.socket().local_addr().unwrap() else {
            unreachable!("a socket bound to IPv4 has an IPv4 address");
        };
        let second = SharedPort::bind(address).unwrap();
        assert_eq!(
            second.socket().local_addr().unwrap(),
            SocketAddr::V4(address)
        );
        let alone = UdpSocket::bind(address).unwrap_err();
        assert_eq!(alone.kind(), ErrorKind::AddrInUse);
    }
}
