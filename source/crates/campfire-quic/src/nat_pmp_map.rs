use std::net::Ipv4Addr;
use std::time::Duration;

use crate::error::ReplyError;

/// NAT-PMP's version (RFC 6886 §3).
const VERSION: u8 = 0;
/// The opcode of the request for the external address; its reply's is 128 more.
const ADDRESS: u8 = 0;
/// The opcode of a UDP mapping; its reply's is 128 more.
const MAP_UDP: u8 = 1;
const REPLY: u8 = 128;
/// A reply's header: version, opcode, result code and epoch, the whole of a refusal.
const HEADER_LEN: usize = 8;
const ADDRESS_REPLY_LEN: usize = 12;
const MAP_REPLY_LEN: usize = 16;

/// A NAT-PMP request for a UDP mapping (RFC 6886 §3.3): `internal_port` outside, at
/// `external_port` where the gateway can, for `lifetime`. A lifetime of 0 deletes the mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NatPmpMap {
    pub(crate) internal_port: u16,
    /// The port to suggest: 0 for none, the assigned one for a renewal.
    pub(crate) external_port: u16,
    pub(crate) lifetime: Duration,
}

/// A gateway's answer to a NAT-PMP request: what it grants, or why it refuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NatPmpReply<T> {
    Granted(T),
    Refused(NatPmpRefusal),
}

/// The mapping a gateway granted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NatPmpMapping {
    pub(crate) internal_port: u16,
    pub(crate) external_port: u16,
    pub(crate) lifetime: Duration,
}

/// The gateway's external address, which a NAT-PMP mapping does not carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NatPmpAddress {
    pub(crate) address: Ipv4Addr,
}

/// Why a gateway refused a request: each result code but Success (RFC 6886 §3.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NatPmpRefusal {
    UnsupportedVersion,
    NotAuthorized,
    NetworkFailure,
    OutOfResources,
    UnsupportedOpcode,
}

impl NatPmpMap {
    /// The request for the gateway's external address (RFC 6886 §3.2).
    pub(crate) const ADDRESS_REQUEST: [u8; 2] = [VERSION, ADDRESS];

    pub(crate) fn encode(&self) -> [u8; 12] {
        let lifetime = u32::try_from(self.lifetime.as_secs())
            .expect("a mapping's lifetime fits NAT-PMP's 32 bits of seconds");
        let mut bytes = [0; 12];
        bytes[0] = VERSION;
        bytes[1] = MAP_UDP;
        bytes[4..6].copy_from_slice(&self.internal_port.to_be_bytes());
        bytes[6..8].copy_from_slice(&self.external_port.to_be_bytes());
        bytes[8..12].copy_from_slice(&lifetime.to_be_bytes());
        bytes
    }

    /// A gateway's reply to a mapping request, from a datagram of untrusted bytes.
    pub(crate) fn decode(bytes: &[u8]) -> Result<NatPmpReply<NatPmpMapping>, ReplyError> {
        if let Some(refusal) = NatPmpMap::refusal(bytes, MAP_UDP)? {
            return Ok(NatPmpReply::Refused(refusal));
        }
        NatPmpMap::require(bytes, MAP_REPLY_LEN)?;
        Ok(NatPmpReply::Granted(NatPmpMapping {
            internal_port: u16::from_be_bytes([bytes[8], bytes[9]]),
            external_port: u16::from_be_bytes([bytes[10], bytes[11]]),
            lifetime: Duration::from_secs(u64::from(NatPmpMap::u32_at(bytes, 12))),
        }))
    }

    /// A gateway's reply to the request for its external address.
    pub(crate) fn decode_address(bytes: &[u8]) -> Result<NatPmpReply<NatPmpAddress>, ReplyError> {
        if let Some(refusal) = NatPmpMap::refusal(bytes, ADDRESS)? {
            return Ok(NatPmpReply::Refused(refusal));
        }
        NatPmpMap::require(bytes, ADDRESS_REPLY_LEN)?;
        Ok(NatPmpReply::Granted(NatPmpAddress {
            address: Ipv4Addr::new(bytes[8], bytes[9], bytes[10], bytes[11]),
        }))
    }

    /// A reply header's refusal, none for success, once its version and opcode answer `opcode`.
    fn refusal(bytes: &[u8], opcode: u8) -> Result<Option<NatPmpRefusal>, ReplyError> {
        NatPmpMap::require(bytes, HEADER_LEN)?;
        if bytes[0] != VERSION {
            return Err(ReplyError::Version(bytes[0]));
        }
        if bytes[1] != REPLY + opcode {
            return Err(ReplyError::Opcode(bytes[1]));
        }
        Ok(match u16::from_be_bytes([bytes[2], bytes[3]]) {
            0 => None,
            1 => Some(NatPmpRefusal::UnsupportedVersion),
            2 => Some(NatPmpRefusal::NotAuthorized),
            3 => Some(NatPmpRefusal::NetworkFailure),
            4 => Some(NatPmpRefusal::OutOfResources),
            5 => Some(NatPmpRefusal::UnsupportedOpcode),
            code => return Err(ReplyError::Result(code)),
        })
    }

    fn require(bytes: &[u8], needed: usize) -> Result<(), ReplyError> {
        if bytes.len() < needed {
            return Err(ReplyError::TooShort {
                len: bytes.len(),
                needed,
            });
        }
        Ok(())
    }

    fn u32_at(bytes: &[u8], at: usize) -> u32 {
        u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_map_request_lays_out_its_fields_as_rfc_6886_does() {
        let request = NatPmpMap {
            internal_port: 4433,
            external_port: 0,
            lifetime: Duration::from_secs(7200),
        };
        // Version 0, opcode 1, 2 reserved bytes, 4,433 (0x1151), no suggested port, 7,200 s
        // (0x1c20).
        assert_eq!(
            request.encode(),
            [0, 1, 0, 0, 0x11, 0x51, 0, 0, 0, 0, 0x1c, 0x20]
        );
        assert_eq!(NatPmpMap::ADDRESS_REQUEST, [0, 0]);
    }

    #[test]
    fn a_reply_grants_or_refuses_and_one_that_does_not_read_is_refused_by_its_flaw() {
        // Opcode 129, result 0, epoch 1,000 s (0x03e8), 4,433 to 50,000 (0xc350), for 3,600 s
        // (0x0e10).
        let map = [
            0, 129, 0, 0, 0, 0, 0x03, 0xe8, 0x11, 0x51, 0xc3, 0x50, 0, 0, 0x0e, 0x10,
        ];
        assert_eq!(
            NatPmpMap::decode(&map),
            Ok(NatPmpReply::Granted(NatPmpMapping {
                internal_port: 4433,
                external_port: 50_000,
                lifetime: Duration::from_secs(3600),
            }))
        );
        // Opcode 128, result 0, epoch 1,000 s, 203.0.113.7.
        let address = [0, 128, 0, 0, 0, 0, 0x03, 0xe8, 203, 0, 113, 7];
        assert_eq!(
            NatPmpMap::decode_address(&address),
            Ok(NatPmpReply::Granted(NatPmpAddress {
                address: Ipv4Addr::new(203, 0, 113, 7),
            }))
        );
        // A refusal is the header alone: result 4, Out of resources.
        assert_eq!(
            NatPmpMap::decode(&[0, 129, 0, 4, 0, 0, 0, 9]),
            Ok(NatPmpReply::Refused(NatPmpRefusal::OutOfResources))
        );
        for (bytes, error) in [
            (&map[..7], ReplyError::TooShort { len: 7, needed: 8 }),
            (
                &map[..15],
                ReplyError::TooShort {
                    len: 15,
                    needed: 16,
                },
            ),
            (&[2, 129, 0, 0, 0, 0, 0, 0][..], ReplyError::Version(2)),
            // The address request's reply, where a mapping's is due.
            (&address[..], ReplyError::Opcode(128)),
            (&[0, 129, 0, 6, 0, 0, 0, 0][..], ReplyError::Result(6)),
        ] {
            assert_eq!(NatPmpMap::decode(bytes), Err(error), "{bytes:?}");
        }
        assert_eq!(
            NatPmpMap::decode_address(&address[..11]),
            Err(ReplyError::TooShort {
                len: 11,
                needed: 12
            })
        );
    }
}
