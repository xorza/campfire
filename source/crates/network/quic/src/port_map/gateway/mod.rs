use std::io::{self, ErrorKind};
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::port_map::error::MapError;
use crate::port_map::facing_socket::FacingSocket;
use crate::port_map::nat_pmp_map::{NatPmpMap, NatPmpMapping, NatPmpReply};
use crate::port_map::pcp_map::{MapNonce, PcpMap, PcpReply};

/// The longest PCP message, which bounds a NAT-PMP one too (RFC 6887 §7).
const MESSAGE_MAX: usize = 1100;
const NO_EXTERNAL: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0);

/// A client of a gateway's port mappings (11-quic-transport.md, A server maps its own port): it
/// speaks PCP, and NAT-PMP to a gateway that answers PCP with NAT-PMP's version, from a socket that
/// only the gateway's replies reach.
#[derive(Debug)]
pub(crate) struct Gateway {
    socket: UdpSocket,
    /// The address of this host on the gateway's side, which a PCP request names.
    client: Ipv4Addr,
    /// The gateway answered PCP with NAT-PMP's version, so each later request speaks NAT-PMP.
    nat_pmp_only: bool,
}

/// A port a gateway maps: its port outside, its router's address outside once read, how long
/// until it is renewed, and by which protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Mapping {
    pub(crate) external_port: u16,
    /// The router's address outside; none for a grant whose address did not read, which is
    /// still the router's to delete.
    pub(crate) address: Option<Ipv4Addr>,
    /// The lifetime the gateway granted; for a UPnP lease that never ends, the one asked for.
    pub(crate) lifetime: Duration,
    pub(crate) by: MappedBy,
}

/// What a map asks of a gateway or a router: `internal_port` for `lifetime`, anew, or `renewed`
/// again at its port outside.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ask<'a> {
    pub(crate) internal_port: u16,
    pub(crate) renewed: Option<&'a Mapping>,
    pub(crate) lifetime: Duration,
}

/// How a request to a gateway waits: its tries, and the map's stop, which ends the wait before
/// the next try.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Wait<'a> {
    pub(crate) tries: Tries,
    pub(crate) stop: &'a AtomicBool,
}

/// The protocol a mapping was granted by, and what its renewal and deletion repeat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MappedBy {
    Pcp(MapNonce),
    NatPmp,
    Upnp,
}

/// How a request is sent again: `count` times, the first answer awaited `first`, each later one
/// twice the last, as RFC 6886 §3.1 and RFC 6887 §8.1.1 both do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Tries {
    pub(crate) first: Duration,
    pub(crate) count: u32,
}

impl Mapping {
    /// Where a player outside reaches the port, once its address read.
    pub(crate) fn external(&self) -> Option<SocketAddrV4> {
        Some(SocketAddrV4::new(self.address?, self.external_port))
    }
}

impl Gateway {
    /// A client of the gateway at `gateway`, from this host's address that faces it.
    pub(crate) fn connect(gateway: SocketAddrV4) -> io::Result<Self> {
        let FacingSocket { socket, host } = FacingSocket::connect(gateway)?;
        Ok(Self {
            socket,
            client: host,
            nat_pmp_only: false,
        })
    }

    /// Maps as `ask` says. A PCP grant names its address outside; a NAT-PMP one does not, and
    /// `address` reads it.
    pub(crate) fn map(&mut self, ask: Ask<'_>, wait: Wait<'_>) -> Result<Mapping, MapError> {
        let Ask {
            internal_port,
            renewed,
            lifetime,
        } = ask;
        if !self.nat_pmp_only {
            let nonce = match renewed {
                Some(Mapping {
                    by: MappedBy::Pcp(nonce),
                    ..
                }) => *nonce,
                _ => MapNonce::random(),
            };
            let request = PcpMap {
                nonce,
                client: self.client,
                internal_port,
                external: renewed.and_then(Mapping::external).unwrap_or(NO_EXTERNAL),
                lifetime,
            };
            match self.pcp(&request, wait)? {
                PcpReply::Granted(granted) => {
                    return Ok(Mapping {
                        external_port: granted.external.port(),
                        address: Some(*granted.external.ip()),
                        lifetime: granted.lifetime,
                        by: MappedBy::Pcp(nonce),
                    });
                }
                PcpReply::Refused { refusal, lasts } => {
                    return Err(MapError::PcpRefused { refusal, lasts });
                }
                PcpReply::NatPmpOnly => self.nat_pmp_only = true,
            }
        }
        let request = NatPmpMap {
            internal_port,
            external_port: renewed.map_or(0, |mapping| mapping.external_port),
            lifetime,
        };
        let granted = match self.nat_pmp(&request, wait)? {
            NatPmpReply::Granted(granted) => granted,
            NatPmpReply::Refused(refusal) => return Err(MapError::NatPmpRefused(refusal)),
        };
        Ok(Mapping {
            external_port: granted.external_port,
            address: None,
            lifetime: granted.lifetime,
            by: MappedBy::NatPmp,
        })
    }

    /// The gateway's address outside, by NAT-PMP.
    pub(crate) fn address(&self, wait: Wait<'_>) -> Result<Ipv4Addr, MapError> {
        let address = self.exchange(&NatPmpMap::ADDRESS_REQUEST, wait, |reply| {
            NatPmpMap::decode_address(reply).ok()
        })?;
        match address {
            NatPmpReply::Granted(address) => Ok(address.address),
            NatPmpReply::Refused(refusal) => Err(MapError::NatPmpRefused(refusal)),
        }
    }

    /// Deletes `mapping` of `internal_port`, which PCP or NAT-PMP granted: one request, its answer
    /// awaited `wait`.
    pub(crate) fn delete(
        &self,
        internal_port: u16,
        mapping: &Mapping,
        wait: Duration,
    ) -> Result<(), MapError> {
        // A deletion comes as the map stops, which no stop cuts short.
        let running = AtomicBool::new(false);
        let once = Wait {
            tries: Tries {
                first: wait,
                count: 1,
            },
            stop: &running,
        };
        match mapping.by {
            MappedBy::Pcp(nonce) => {
                let request = PcpMap {
                    nonce,
                    client: self.client,
                    internal_port,
                    external: NO_EXTERNAL,
                    lifetime: Duration::ZERO,
                };
                match self.pcp(&request, once)? {
                    PcpReply::Granted(_) => Ok(()),
                    PcpReply::Refused { refusal, lasts } => {
                        Err(MapError::PcpRefused { refusal, lasts })
                    }
                    PcpReply::NatPmpOnly => Err(MapError::NatPmpOnly),
                }
            }
            MappedBy::NatPmp => {
                let request = NatPmpMap {
                    internal_port,
                    external_port: 0,
                    lifetime: Duration::ZERO,
                };
                match self.nat_pmp(&request, once)? {
                    NatPmpReply::Granted(_) => Ok(()),
                    NatPmpReply::Refused(refusal) => Err(MapError::NatPmpRefused(refusal)),
                }
            }
            MappedBy::Upnp => unreachable!("a UPnP mapping is deleted through its router"),
        }
    }

    /// A PCP request's answer: a reply that grants another mapping than the request's, or that
    /// does not read, answers some other request.
    fn pcp(&self, request: &PcpMap, wait: Wait<'_>) -> Result<PcpReply, MapError> {
        self.exchange(&request.encode(), wait, |reply| {
            match PcpReply::decode(reply) {
                Ok(PcpReply::Granted(granted))
                    if granted.nonce != request.nonce
                        || granted.internal_port != request.internal_port =>
                {
                    None
                }
                reply => reply.ok(),
            }
        })
    }

    fn nat_pmp(
        &self,
        request: &NatPmpMap,
        wait: Wait<'_>,
    ) -> Result<NatPmpReply<NatPmpMapping>, MapError> {
        self.exchange(&request.encode(), wait, |reply| {
            match NatPmpMap::decode(reply) {
                Ok(NatPmpReply::Granted(granted))
                    if granted.internal_port != request.internal_port =>
                {
                    None
                }
                reply => reply.ok(),
            }
        })
    }

    /// Sends `request` as `wait` says, until `answer` takes a reply as its answer, or the map
    /// stops before a try.
    fn exchange<T>(
        &self,
        request: &[u8],
        wait: Wait<'_>,
        mut answer: impl FnMut(&[u8]) -> Option<T>,
    ) -> Result<T, MapError> {
        let mut buffer = [0; MESSAGE_MAX];
        let mut awaited = wait.tries.first;
        for _ in 0..wait.tries.count {
            if wait.stop.load(Ordering::SeqCst) {
                return Err(MapError::Stopped);
            }
            self.socket.send(request).map_err(Self::fault)?;
            let deadline = Instant::now() + awaited;
            loop {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    break;
                }
                self.socket
                    .set_read_timeout(Some(left))
                    .map_err(MapError::Io)?;
                match self.socket.recv(&mut buffer) {
                    Ok(len) => {
                        if let Some(answer) = answer(&buffer[..len]) {
                            return Ok(answer);
                        }
                    }
                    Err(error)
                        if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
                    {
                        break;
                    }
                    Err(error) => return Err(Self::fault(error)),
                }
            }
            awaited *= 2;
        }
        Err(MapError::NoAnswer)
    }

    /// A socket error as the mapping's: the ICMP answer of a port no server listens on reaches a
    /// connected socket as a refused or reset connection, as each OS names it.
    fn fault(error: io::Error) -> MapError {
        match error.kind() {
            ErrorKind::ConnectionRefused | ErrorKind::ConnectionReset => MapError::NoServer,
            _ => MapError::Io(error),
        }
    }
}

#[cfg(test)]
pub(crate) mod fake_gateway;

#[cfg(test)]
mod tests;
