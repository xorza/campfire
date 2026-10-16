use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4};
use std::time::Duration;

use ring::rand::{SecureRandom, SystemRandom};

use crate::reply_error::ReplyError;

/// PCP's version; a gateway that speaks NAT-PMP alone answers with its own, 0 (RFC 6887 §9).
const VERSION: u8 = 2;
const MAP: u8 = 1;
/// The opcode octet's high bit, set in a reply.
const REPLY: u8 = 0x80;
const UDP: u8 = 17;
/// A header's length, the whole of a reply that refuses.
const HEADER_LEN: usize = 24;
/// A MAP request's and a granting reply's length: the header and a body of 36 (RFC 6887 §11.1).
pub(crate) const MAP_LEN: usize = 60;

/// A PCP MAP request for a UDP port (RFC 6887 §11.1): the client at `client` asks its gateway for
/// `internal_port` outside, at `external` where it can, for `lifetime`. A lifetime of 0 deletes
/// the mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PcpMap {
    pub(crate) nonce: MapNonce,
    pub(crate) client: Ipv4Addr,
    pub(crate) internal_port: u16,
    /// The external address to suggest: `0.0.0.0:0` for none, the assigned one for a renewal.
    pub(crate) external: SocketAddrV4,
    pub(crate) lifetime: Duration,
}

/// The random value that names a mapping to its gateway, the same in its renewals and its
/// deletion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MapNonce([u8; 12]);

/// A gateway's answer to a MAP request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PcpReply {
    Granted(PcpMapping),
    /// The gateway refused, and expects the same answer for `lasts`.
    Refused {
        refusal: PcpRefusal,
        lasts: Duration,
    },
    /// The gateway speaks NAT-PMP alone: it answered with version 0 (RFC 6887 §9).
    NatPmpOnly,
}

/// The mapping a gateway granted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PcpMapping {
    pub(crate) nonce: MapNonce,
    pub(crate) internal_port: u16,
    pub(crate) external: SocketAddrV4,
    pub(crate) lifetime: Duration,
}

/// Why a gateway refused a request: each result code but SUCCESS (RFC 6887 §7.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PcpRefusal {
    UnsupportedVersion,
    NotAuthorized,
    MalformedRequest,
    UnsupportedOpcode,
    UnsupportedOption,
    MalformedOption,
    NetworkFailure,
    NoResources,
    UnsupportedProtocol,
    UserExceededQuota,
    CannotProvideExternal,
    AddressMismatch,
    ExcessiveRemotePeers,
}

impl PcpMap {
    pub(crate) fn encode(&self) -> [u8; MAP_LEN] {
        let lifetime = u32::try_from(self.lifetime.as_secs())
            .expect("a mapping's lifetime fits PCP's 32 bits of seconds");
        let mut bytes = [0; MAP_LEN];
        bytes[0] = VERSION;
        bytes[1] = MAP;
        bytes[4..8].copy_from_slice(&lifetime.to_be_bytes());
        bytes[8..24].copy_from_slice(&self.client.to_ipv6_mapped().octets());
        bytes[24..36].copy_from_slice(&self.nonce.0);
        bytes[36] = UDP;
        bytes[40..42].copy_from_slice(&self.internal_port.to_be_bytes());
        bytes[42..44].copy_from_slice(&self.external.port().to_be_bytes());
        bytes[44..60].copy_from_slice(&self.external.ip().to_ipv6_mapped().octets());
        bytes
    }
}

impl MapNonce {
    pub(crate) fn random() -> Self {
        let mut nonce = [0; 12];
        SystemRandom::new()
            .fill(&mut nonce)
            .expect("the OS gives random bytes");
        Self(nonce)
    }
}

impl PcpReply {
    /// A gateway's reply to a MAP request, from a datagram of untrusted bytes.
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, ReplyError> {
        let too_short = |needed| ReplyError::TooShort {
            len: bytes.len(),
            needed,
        };
        match bytes.first() {
            Some(0) => return Ok(Self::NatPmpOnly),
            Some(&VERSION) => {}
            Some(&version) => return Err(ReplyError::Version(version)),
            None => return Err(too_short(HEADER_LEN)),
        }
        if bytes.len() < HEADER_LEN {
            return Err(too_short(HEADER_LEN));
        }
        if bytes[1] != REPLY | MAP {
            return Err(ReplyError::Opcode(bytes[1]));
        }
        let lifetime = Duration::from_secs(u64::from(Self::u32_at(bytes, 4)));
        if let Some(refusal) = PcpRefusal::of(bytes[3])? {
            return Ok(Self::Refused {
                refusal,
                lasts: lifetime,
            });
        }
        if bytes.len() < MAP_LEN {
            return Err(too_short(MAP_LEN));
        }
        if bytes[36] != UDP {
            return Err(ReplyError::Protocol(bytes[36]));
        }
        let mut nonce = [0; 12];
        nonce.copy_from_slice(&bytes[24..36]);
        let mut address = [0; 16];
        address.copy_from_slice(&bytes[44..60]);
        let address = Ipv6Addr::from(address)
            .to_ipv4_mapped()
            .ok_or(ReplyError::NotIpv4)?;
        Ok(Self::Granted(PcpMapping {
            nonce: MapNonce(nonce),
            internal_port: u16::from_be_bytes([bytes[40], bytes[41]]),
            external: SocketAddrV4::new(address, u16::from_be_bytes([bytes[42], bytes[43]])),
            lifetime,
        }))
    }

    const fn u32_at(bytes: &[u8], at: usize) -> u32 {
        u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
    }
}

impl PcpRefusal {
    /// The refusal a result code names, none for SUCCESS.
    fn of(code: u8) -> Result<Option<Self>, ReplyError> {
        Ok(Some(match code {
            0 => return Ok(None),
            1 => Self::UnsupportedVersion,
            2 => Self::NotAuthorized,
            3 => Self::MalformedRequest,
            4 => Self::UnsupportedOpcode,
            5 => Self::UnsupportedOption,
            6 => Self::MalformedOption,
            7 => Self::NetworkFailure,
            8 => Self::NoResources,
            9 => Self::UnsupportedProtocol,
            10 => Self::UserExceededQuota,
            11 => Self::CannotProvideExternal,
            12 => Self::AddressMismatch,
            13 => Self::ExcessiveRemotePeers,
            code => return Err(ReplyError::Result(u16::from(code))),
        }))
    }
}

#[cfg(test)]
mod tests;
