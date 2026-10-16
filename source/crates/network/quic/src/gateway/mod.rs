use std::io::{self, ErrorKind};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::time::{Duration, Instant};

use crate::error::MapError;
use crate::nat_pmp_map::{NatPmpMap, NatPmpMapping, NatPmpReply};
use crate::pcp_map::{MapNonce, PcpMap, PcpReply};

/// The longest PCP message, which bounds a NAT-PMP one too (RFC 6887 §7).
const MESSAGE_MAX: usize = 1100;
const NO_EXTERNAL: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0);

/// A client of a gateway's port mappings (design 15, D12): it speaks PCP, and NAT-PMP to a
/// gateway that answers PCP with NAT-PMP's version, from a socket that only the gateway's replies
/// reach.
#[derive(Debug)]
pub(crate) struct Gateway {
    socket: UdpSocket,
    /// The address of this host on the gateway's side, which a PCP request names.
    client: Ipv4Addr,
    /// The gateway answered PCP with NAT-PMP's version, so each later request speaks NAT-PMP.
    nat_pmp_only: bool,
}

/// A port a gateway maps: where it is reached outside, how long until it is renewed, and by which
/// protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Mapping {
    pub(crate) external: SocketAddrV4,
    /// The lifetime the gateway granted; for a UPnP lease that never ends, the one asked for.
    pub(crate) lifetime: Duration,
    pub(crate) by: MappedBy,
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

impl Gateway {
    /// A client of the gateway at `gateway`, from this host's address that faces it.
    pub(crate) fn connect(gateway: SocketAddrV4) -> io::Result<Self> {
        let socket = UdpSocket::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)))?;
        socket.connect(gateway)?;
        let SocketAddr::V4(local) = socket.local_addr()? else {
            unreachable!("a socket bound to an IPv4 address keeps one");
        };
        Ok(Self {
            socket,
            client: *local.ip(),
            nat_pmp_only: false,
        })
    }

    /// Maps `internal_port` for `lifetime`: anew, or `renewed` again at its external address.
    pub(crate) fn map(
        &mut self,
        internal_port: u16,
        renewed: Option<&Mapping>,
        lifetime: Duration,
        tries: Tries,
    ) -> Result<Mapping, MapError> {
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
                external: renewed.map_or(NO_EXTERNAL, |mapping| mapping.external),
                lifetime,
            };
            match self.pcp(&request, tries)? {
                PcpReply::Granted(granted) => {
                    return Ok(Mapping {
                        external: granted.external,
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
            external_port: renewed.map_or(0, |mapping| mapping.external.port()),
            lifetime,
        };
        let granted = match self.nat_pmp(&request, tries)? {
            NatPmpReply::Granted(granted) => granted,
            NatPmpReply::Refused(refusal) => return Err(MapError::NatPmpRefused(refusal)),
        };
        let address = self.exchange(&NatPmpMap::ADDRESS_REQUEST, tries, |reply| {
            NatPmpMap::decode_address(reply).ok()
        })?;
        let address = match address {
            NatPmpReply::Granted(address) => address,
            NatPmpReply::Refused(refusal) => return Err(MapError::NatPmpRefused(refusal)),
        };
        Ok(Mapping {
            external: SocketAddrV4::new(address.address, granted.external_port),
            lifetime: granted.lifetime,
            by: MappedBy::NatPmp,
        })
    }

    /// Deletes `mapping` of `internal_port`, which PCP or NAT-PMP granted: one request, its answer
    /// awaited `wait`.
    pub(crate) fn delete(
        &self,
        internal_port: u16,
        mapping: &Mapping,
        wait: Duration,
    ) -> Result<(), MapError> {
        let once = Tries {
            first: wait,
            count: 1,
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
    fn pcp(&self, request: &PcpMap, tries: Tries) -> Result<PcpReply, MapError> {
        self.exchange(&request.encode(), tries, |reply| {
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
        tries: Tries,
    ) -> Result<NatPmpReply<NatPmpMapping>, MapError> {
        self.exchange(&request.encode(), tries, |reply| {
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

    /// Sends `request` as `tries` says, until `answer` takes a reply as its answer.
    fn exchange<T>(
        &self,
        request: &[u8],
        tries: Tries,
        mut answer: impl FnMut(&[u8]) -> Option<T>,
    ) -> Result<T, MapError> {
        let mut buffer = [0; MESSAGE_MAX];
        let mut wait = tries.first;
        for _ in 0..tries.count {
            self.socket.send(request).map_err(Self::fault)?;
            let deadline = Instant::now() + wait;
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
            wait *= 2;
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
