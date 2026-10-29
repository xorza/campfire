use std::io;
use std::net::{SocketAddr, UdpSocket};

use socket2::{Domain, Protocol, Socket, Type};

/// A UDP socket whose IPv6 address takes IPv4 peers too, as their mapped addresses, on every OS:
/// `IPV6_V6ONLY` off, which Linux and macOS default to and Windows does not, so `[::]` means every
/// address of the host wherever it runs (11-net-stack/udp.md, An IPv6 socket is dual-stack). A
/// socket bound to IPv4 takes IPv4 alone.
#[derive(Debug)]
pub struct DualStack;

impl DualStack {
    pub fn bind(address: SocketAddr) -> io::Result<UdpSocket> {
        let socket = Socket::new(
            Domain::for_address(address),
            Type::DGRAM,
            Some(Protocol::UDP),
        )?;
        if address.is_ipv6() {
            socket.set_only_v6(false)?;
        }
        socket.bind(&address.into())?;
        Ok(socket.into())
    }
}

#[cfg(test)]
mod tests;
