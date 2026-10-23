use std::io::{self, ErrorKind};
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::error::PortMapError;
use crate::facing_socket::FacingSocket;
use crate::mapping::{MapAsk, MappedBy, Mapping};
use crate::nat_pmp_map::{NatPmpMap, NatPmpMapping, NatPmpReply};
use crate::pcp_map::{MapNonce, PcpMap, PcpReply};

/// The longest PCP message, which bounds a NAT-PMP one too (RFC 6887 §7).
const MESSAGE_MAX: usize = 1100;
const NO_EXTERNAL: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0);

/// A client of a gateway's port mappings (11-net-stack/port-map.md, PCP, then NAT-PMP, then
/// UPnP-IGD): it speaks PCP, and NAT-PMP to a gateway that answers PCP with NAT-PMP's version, from
/// a socket that only the gateway's replies reach.
#[derive(Debug)]
pub(crate) struct Gateway {
    socket: UdpSocket,
    /// The address of this host on the gateway's side, which a PCP request names.
    client: Ipv4Addr,
    /// What names the one mapping this client makes to a PCP gateway, in its renewals and its
    /// deletion.
    nonce: MapNonce,
    /// The gateway answered PCP with NAT-PMP's version, so each later request speaks NAT-PMP.
    nat_pmp_only: bool,
}

/// How a request to a gateway waits: its tries, and the map's stop, which ends the wait before
/// the next try.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Wait<'a> {
    pub(crate) tries: Tries,
    pub(crate) stop: &'a AtomicBool,
}

/// How a request is sent again: `count` times, the first answer awaited `first`, each later one
/// twice the last, as RFC 6886 §3.1 and RFC 6887 §8.1.1 both do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Tries {
    pub(crate) first: Duration,
    pub(crate) count: u32,
}

impl Gateway {
    /// A client of the gateway at `gateway`, from this host's address that faces it, whose
    /// mapping `nonce` names.
    pub(crate) fn connect(gateway: SocketAddrV4, nonce: MapNonce) -> io::Result<Self> {
        let FacingSocket { socket, host } = FacingSocket::connect(gateway)?;
        Ok(Self {
            socket,
            client: host,
            nonce,
            nat_pmp_only: false,
        })
    }

    /// Maps as `ask` says. A PCP grant names its address outside; a NAT-PMP one does not, and
    /// `address` reads it.
    pub(crate) fn map(&mut self, ask: MapAsk<'_>, wait: Wait<'_>) -> Result<Mapping, PortMapError> {
        let MapAsk {
            internal_port,
            renewed,
            lifetime,
            ..
        } = ask;
        if !self.nat_pmp_only {
            let request = PcpMap {
                nonce: self.nonce,
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
                        by: MappedBy::Pcp,
                    });
                }
                PcpReply::Refused { refusal, lasts } => {
                    return Err(PortMapError::PcpRefused { refusal, lasts });
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
            NatPmpReply::Refused(refusal) => return Err(PortMapError::NatPmpRefused(refusal)),
        };
        Ok(Mapping {
            external_port: granted.external_port,
            address: None,
            lifetime: granted.lifetime,
            by: MappedBy::NatPmp,
        })
    }

    /// The gateway's address outside, by NAT-PMP.
    pub(crate) fn address(&self, wait: Wait<'_>) -> Result<Ipv4Addr, PortMapError> {
        let address = self.exchange(&NatPmpMap::ADDRESS_REQUEST, wait, |reply| {
            NatPmpMap::decode_address(reply).ok()
        })?;
        match address {
            NatPmpReply::Granted(address) => Ok(address.address),
            NatPmpReply::Refused(refusal) => Err(PortMapError::NatPmpRefused(refusal)),
        }
    }

    /// Deletes the mapping of `internal_port` by the protocol the gateway speaks, which granted
    /// the mapping it holds, as a renewal maps again by NAT-PMP once PCP says it speaks NAT-PMP
    /// alone: one request, its answer awaited `wait`.
    pub(crate) fn delete(&self, internal_port: u16, wait: Duration) -> Result<(), PortMapError> {
        // A deletion comes as the map stops, which no stop cuts short.
        let running = AtomicBool::new(false);
        let once = Wait {
            tries: Tries {
                first: wait,
                count: 1,
            },
            stop: &running,
        };
        if self.nat_pmp_only {
            let request = NatPmpMap {
                internal_port,
                external_port: 0,
                lifetime: Duration::ZERO,
            };
            match self.nat_pmp(&request, once)? {
                NatPmpReply::Granted(_) => Ok(()),
                NatPmpReply::Refused(refusal) => Err(PortMapError::NatPmpRefused(refusal)),
            }
        } else {
            let request = PcpMap {
                nonce: self.nonce,
                client: self.client,
                internal_port,
                external: NO_EXTERNAL,
                lifetime: Duration::ZERO,
            };
            match self.pcp(&request, once)? {
                PcpReply::Granted(_) => Ok(()),
                PcpReply::Refused { refusal, lasts } => {
                    Err(PortMapError::PcpRefused { refusal, lasts })
                }
                PcpReply::NatPmpOnly => Err(PortMapError::NatPmpOnly),
            }
        }
    }

    /// A PCP request's answer: a reply that grants another mapping than the request's, or that
    /// does not read, answers some other request.
    fn pcp(&self, request: &PcpMap, wait: Wait<'_>) -> Result<PcpReply, PortMapError> {
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
    ) -> Result<NatPmpReply<NatPmpMapping>, PortMapError> {
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
    ) -> Result<T, PortMapError> {
        let mut buffer = [0; MESSAGE_MAX];
        let mut awaited = wait.tries.first;
        for _ in 0..wait.tries.count {
            if wait.stop.load(Ordering::SeqCst) {
                return Err(PortMapError::Stopped);
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
                    .map_err(PortMapError::Io)?;
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
        Err(PortMapError::NoAnswer)
    }

    /// A socket error as the mapping's: the ICMP answer of a port no server listens on reaches a
    /// connected socket as a refused or reset connection, as each OS names it.
    fn fault(error: io::Error) -> PortMapError {
        match error.kind() {
            ErrorKind::ConnectionRefused | ErrorKind::ConnectionReset => PortMapError::NoServer,
            _ => PortMapError::Io(error),
        }
    }
}

#[cfg(test)]
pub(crate) mod fake_gateway;

#[cfg(test)]
mod tests;
