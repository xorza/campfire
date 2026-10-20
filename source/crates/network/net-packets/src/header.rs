//! The header of a packet session packet: a byte of flags, the low 16 bits of its number, and, on
//! the types that carry them, its acknowledgements. The header is the AEAD's associated data, so it
//! is authenticated and not encrypted (11-net-stack/packet-session.md, The header is the AEAD's
//! associated data: authenticated, not encrypted).

use campfire_net_payload::PayloadKind;
use core::time::Duration;

use crate::header_error::HeaderError;

/// The first bit, which the packet session leaves 0 and its IO uses for packets of its own
/// (11-net-stack/packet-session.md, The first bit of each datagram is the IO's).
pub(crate) const IO_BIT: u8 = 0x80;
const TYPE_SHIFT: u8 = 4;
const TYPE_MASK: u8 = 0x70;
const KIND_SHIFT: u8 = 2;
const KIND_MASK: u8 = 0x0c;
const ACKS_BIT: u8 = 0x02;
const RESERVED_BIT: u8 = 0x01;

/// The span of time one unit of the ACK delay field is (11-net-stack/packet-session.md, Each ACK
/// says how long its largest waited).
const ACK_DELAY_UNIT: Duration = Duration::from_micros(250);

/// What a packet session packet is, in the header's 3 type bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PacketType {
    /// The frame's payload, of the kind the header names.
    Payload,
    /// Acknowledgements and no payload.
    AckOnly,
    /// No payload: the keep-alive, and the probe a timeout sends.
    Ping,
    /// 8 random bytes a new path echoes.
    PathChallenge,
    /// The 8 bytes of a challenge, echoed.
    PathResponse,
    /// The `AppCode` of the end's close, in 8 bytes.
    Close,
}

impl PacketType {
    const ALL: [Self; 6] = [
        Self::Payload,
        Self::AckOnly,
        Self::Ping,
        Self::PathChallenge,
        Self::PathResponse,
        Self::Close,
    ];

    const fn bits(self) -> u8 {
        match self {
            Self::Payload => 0,
            Self::AckOnly => 1,
            Self::Ping => 2,
            Self::PathChallenge => 3,
            Self::PathResponse => 4,
            Self::Close => 5,
        }
    }

    /// Whether the peer acknowledges a packet of this type, as it acknowledges no packet whose
    /// only job is to acknowledge or to end.
    pub(crate) const fn is_ack_eliciting(self) -> bool {
        !matches!(self, Self::AckOnly | Self::Close)
    }

    /// Whether the type carries acknowledgements: always an `AckOnly`, and a `Payload` once its
    /// sender accepted a packet.
    const fn takes_acks(self) -> bool {
        matches!(self, Self::Payload | Self::AckOnly)
    }
}

/// The acknowledgements a header carries: the low 16 bits of the largest accepted packet, a bit
/// for each of the 32 numbers below it, the nearest in the lowest bit, and how long the largest
/// waited for this acknowledgement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AckFields {
    pub(crate) largest: u16,
    pub(crate) bits: u32,
    delay: u8,
}

impl AckFields {
    pub(crate) const fn new(largest: u16, bits: u32, delay: Duration) -> Self {
        let units = delay.as_micros() / ACK_DELAY_UNIT.as_micros();
        Self {
            largest,
            bits,
            delay: if units > u8::MAX as u128 {
                u8::MAX
            } else {
                units as u8
            },
        }
    }

    /// The ACK delay, in whole units of 0.25 ms, at most 63.75 ms.
    pub(crate) const fn delay(self) -> Duration {
        Duration::from_micros(self.delay as u64 * ACK_DELAY_UNIT.as_micros() as u64)
    }
}

/// A packet session packet's header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Header {
    pub(crate) packet_type: PacketType,
    /// The payload's kind; 0 on a type that carries no frame payload.
    pub(crate) kind: PayloadKind,
    pub(crate) number: u16,
    pub(crate) acks: Option<AckFields>,
}

impl Header {
    /// The longest header: flags, number and acknowledgements.
    pub(crate) const MAX_LEN: usize = 1 + 2 + 7;

    pub(crate) const fn len(&self) -> usize {
        if self.acks.is_some() { 10 } else { 3 }
    }

    /// Writes the header into `out`, exactly `len` bytes.
    pub(crate) fn write(&self, out: &mut [u8]) {
        debug_assert_eq!(out.len(), self.len(), "a header fills its own bytes");
        debug_assert!(
            self.is_well_formed(),
            "a packet session writes only headers it reads"
        );
        let mut flags = (self.packet_type.bits() << TYPE_SHIFT) | (self.kind.bits() << KIND_SHIFT);
        if self.acks.is_some() {
            flags |= ACKS_BIT;
        }
        out[0] = flags;
        out[1..3].copy_from_slice(&self.number.to_be_bytes());
        if let Some(acks) = self.acks {
            out[3..5].copy_from_slice(&acks.largest.to_be_bytes());
            out[5..9].copy_from_slice(&acks.bits.to_be_bytes());
            out[9] = acks.delay;
        }
    }

    /// Reads the header at the start of `bytes`; its `len` tells where it ends.
    pub(crate) fn read(bytes: &[u8]) -> Result<Self, HeaderError> {
        let [flags, high, low, rest @ ..] = bytes else {
            return Err(HeaderError::Truncated);
        };
        if flags & IO_BIT != 0 {
            return Err(HeaderError::IoPacket);
        }
        if flags & RESERVED_BIT != 0 {
            return Err(HeaderError::Reserved);
        }
        let type_bits = (flags & TYPE_MASK) >> TYPE_SHIFT;
        let packet_type = PacketType::ALL
            .into_iter()
            .find(|packet_type| packet_type.bits() == type_bits)
            .ok_or(HeaderError::UnknownType(type_bits))?;
        let acks = if flags & ACKS_BIT == 0 {
            None
        } else {
            let [l0, l1, b0, b1, b2, b3, delay, ..] = *rest else {
                return Err(HeaderError::Truncated);
            };
            Some(AckFields {
                largest: u16::from_be_bytes([l0, l1]),
                bits: u32::from_be_bytes([b0, b1, b2, b3]),
                delay,
            })
        };
        let header = Self {
            packet_type,
            kind: PayloadKind::new((flags & KIND_MASK) >> KIND_SHIFT),
            number: u16::from_be_bytes([*high, *low]),
            acks,
        };
        if !header.is_well_formed() {
            return Err(HeaderError::Shape);
        }
        Ok(header)
    }

    /// Whether the header has its type's shape: a payload kind only on a `Payload`, and
    /// acknowledgements on each `AckOnly` and on no type but it and `Payload`.
    const fn is_well_formed(&self) -> bool {
        let kind = matches!(self.packet_type, PacketType::Payload) || self.kind.bits() == 0;
        let acks = match self.packet_type {
            PacketType::AckOnly => self.acks.is_some(),
            packet_type => packet_type.takes_acks() || self.acks.is_none(),
        };
        kind && acks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acks() -> AckFields {
        AckFields::new(0xbeef, 0x8000_0001, Duration::from_micros(1_250))
    }

    /// Each well-formed header writes the bytes it reads back: 10 with acknowledgements, 3
    /// without. A payload of kind 2 with acks of 0xbeef, bits 0x80000001 and 1.25 ms (5 units)
    /// is `0b0000_1010`, the number, the largest, the bits and 5.
    #[test]
    fn a_header_reads_what_it_writes() {
        let payload = Header {
            packet_type: PacketType::Payload,
            kind: PayloadKind::new(2),
            number: 0x1234,
            acks: Some(acks()),
        };
        let mut bytes = [0; Header::MAX_LEN];
        payload.write(&mut bytes);
        assert_eq!(
            bytes,
            [
                0b0000_1010,
                0x12,
                0x34,
                0xbe,
                0xef,
                0x80,
                0x00,
                0x00,
                0x01,
                5
            ]
        );
        assert_eq!(Header::read(&bytes), Ok(payload));
        for packet_type in PacketType::ALL {
            let acks = matches!(packet_type, PacketType::AckOnly).then(acks);
            let header = Header {
                packet_type,
                kind: PayloadKind::new(0),
                number: 7,
                acks,
            };
            let mut bytes = vec![0; header.len()];
            header.write(&mut bytes);
            assert_eq!(bytes.len(), if acks.is_some() { 10 } else { 3 });
            assert_eq!(Header::read(&bytes), Ok(header), "{packet_type:?}");
        }
    }

    /// The ACK delay counts whole units of 0.25 ms: 1.4 ms is 5 units, 1.25 ms; 64 ms is past
    /// the 255 units of 63.75 ms the field holds.
    #[test]
    fn the_ack_delay_counts_quarter_milliseconds_up_to_its_byte() {
        let delay = |micros| AckFields::new(0, 0, Duration::from_micros(micros)).delay();
        assert_eq!(delay(1_400), Duration::from_micros(1_250));
        assert_eq!(delay(64_000), Duration::from_micros(63_750));
    }

    /// A datagram that is not a well-formed header is refused, each for its reason.
    #[test]
    fn a_malformed_header_is_refused() {
        assert_eq!(Header::read(&[0, 0]), Err(HeaderError::Truncated));
        assert_eq!(Header::read(&[IO_BIT, 0, 0]), Err(HeaderError::IoPacket));
        assert_eq!(
            Header::read(&[RESERVED_BIT, 0, 0]),
            Err(HeaderError::Reserved)
        );
        assert_eq!(
            Header::read(&[0x60, 0, 0]),
            Err(HeaderError::UnknownType(6))
        );
        // Acknowledgements announced and cut short.
        assert_eq!(
            Header::read(&[ACKS_BIT, 0, 0, 0]),
            Err(HeaderError::Truncated)
        );
        // An ACK-only packet with no acknowledgements; a ping with a payload kind; a ping with
        // acknowledgements.
        for flags in [0x10, 0x20 | 0x04, 0x20 | ACKS_BIT] {
            assert_eq!(
                Header::read(&[flags, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
                Err(HeaderError::Shape),
                "{flags:#x}"
            );
        }
    }
}
