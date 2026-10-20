//! The handshake's packets, whose first bit is the IO's: a type, a version, and the client's
//! handshake ID, which routes a server's pending handshake. The first key epoch goes in the clear
//! in `Initial` packets, as TLS over TCP sends it, and the transcript hash protects it; the
//! handshake epoch is sealed in `Handshake` packets (11-net-stack/udp.md, The handshake is TLS 1.3
//! by rustls's QUIC API).

use bytes::{BufMut, Bytes, BytesMut};
use campfire_net_packets::TAG_LEN;
use campfire_net_payload::DEFAULT_MTU;
use rustls::quic::PacketKey;

use crate::address_token::TOKEN_LEN;

/// The protocol version each handshake packet names.
pub(crate) const VERSION: u8 = 1;

/// The bytes of a client's handshake ID.
pub(crate) const ID_LEN: usize = 8;

/// The largest datagram of a handshake, the fixed MTU.
pub(crate) const MAX_DATAGRAM: usize = DEFAULT_MTU;

/// A client's `Initial` is padded to this, so no answer of a server to an address it has not
/// checked is larger than what it received (11-net-stack/udp.md, A Retry checks each new address).
pub(crate) const PADDED_INITIAL: usize = MAX_DATAGRAM;

/// The long header: the type byte, the version and the ID.
const HEADER_LEN: usize = 2 + ID_LEN;

/// A crypto frame's offset and length.
const FRAME_LEN: usize = 4 + 2;

/// A handshake packet's number, which its nonce takes.
const NUMBER_LEN: usize = 2;

/// The most handshake data one `Initial` carries, with its token.
pub(crate) const INITIAL_DATA: usize = MAX_DATAGRAM - HEADER_LEN - 1 - TOKEN_LEN - FRAME_LEN;

/// The most handshake data one `Handshake` packet carries.
pub(crate) const HANDSHAKE_DATA: usize =
    MAX_DATAGRAM - HEADER_LEN - NUMBER_LEN - FRAME_LEN - TAG_LEN;

/// What a handshake packet is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PacketType {
    Initial,
    Retry,
    Handshake,
    Refuse,
    Alert,
}

impl PacketType {
    const fn byte(self) -> u8 {
        let bits = match self {
            Self::Initial => 0,
            Self::Retry => 1,
            Self::Handshake => 2,
            Self::Refuse => 3,
            Self::Alert => 4,
        };
        0x80 | (bits << 4)
    }

    const fn of(byte: u8) -> Option<Self> {
        if byte & 0x8f != 0x80 {
            return None;
        }
        Some(match (byte >> 4) & 0x07 {
            0 => Self::Initial,
            1 => Self::Retry,
            2 => Self::Handshake,
            3 => Self::Refuse,
            4 => Self::Alert,
            _ => return None,
        })
    }
}

/// A handshake packet, read.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum HandshakePacket {
    /// Handshake data of the first epoch, from `offset`, and a client's address token.
    Initial {
        id: [u8; ID_LEN],
        token: Option<[u8; TOKEN_LEN]>,
        offset: u32,
        data: Bytes,
    },
    /// The server's Retry, with the token the client's next `Initial` carries.
    Retry {
        id: [u8; ID_LEN],
        token: [u8; TOKEN_LEN],
    },
    /// Sealed handshake data: `datagram`, numbered `number`, opens with the handshake epoch's key.
    Handshake {
        id: [u8; ID_LEN],
        number: u16,
        datagram: BytesMut,
    },
    /// The server takes no handshake from the address now.
    Refuse { id: [u8; ID_LEN] },
    /// The peer's TLS failed with this alert.
    Alert { id: [u8; ID_LEN], alert: u8 },
}

impl HandshakePacket {
    /// Reads `datagram`, whose first bit is the IO's; `None` for one that is not a handshake
    /// packet of this version.
    pub(crate) fn read(datagram: BytesMut) -> Option<Self> {
        let [first, version, ..] = datagram[..] else {
            return None;
        };
        let packet_type = PacketType::of(first)?;
        if version != VERSION || datagram.len() < HEADER_LEN {
            return None;
        }
        let id: [u8; ID_LEN] = datagram[2..HEADER_LEN].try_into().ok()?;
        let body = &datagram[HEADER_LEN..];
        Some(match packet_type {
            PacketType::Initial => {
                let (&has_token, rest) = body.split_first()?;
                let (token, rest) = match has_token {
                    0 => (None, rest),
                    1 => {
                        let token = rest.get(..TOKEN_LEN)?.try_into().ok()?;
                        (Some(token), &rest[TOKEN_LEN..])
                    }
                    _ => return None,
                };
                let frame = Self::frame(rest)?;
                let start = datagram.len() - rest.len() + FRAME_LEN;
                Self::Initial {
                    id,
                    token,
                    offset: frame.offset,
                    data: datagram.freeze().slice(start..start + frame.len),
                }
            }
            PacketType::Retry => Self::Retry {
                id,
                token: body.get(..TOKEN_LEN)?.try_into().ok()?,
            },
            PacketType::Handshake => {
                let number = u16::from_be_bytes(body.get(..NUMBER_LEN)?.try_into().ok()?);
                if body.len() < NUMBER_LEN + FRAME_LEN + TAG_LEN {
                    return None;
                }
                Self::Handshake {
                    id,
                    number,
                    datagram,
                }
            }
            PacketType::Refuse => Self::Refuse { id },
            PacketType::Alert => Self::Alert {
                id,
                alert: *body.first()?,
            },
        })
    }

    /// The crypto frame at the start of `bytes`, whose data `bytes` holds.
    fn frame(bytes: &[u8]) -> Option<FrameSpan> {
        let offset = u32::from_be_bytes(bytes.get(..4)?.try_into().ok()?);
        let len = usize::from(u16::from_be_bytes(bytes.get(4..6)?.try_into().ok()?));
        (bytes.len() >= FRAME_LEN + len).then_some(FrameSpan { offset, len })
    }

    /// Opens a `Handshake` packet's `datagram` with `key` in place: its crypto frame, or `None`
    /// when it does not authenticate.
    pub(crate) fn open(
        number: u16,
        mut datagram: BytesMut,
        key: &dyn PacketKey,
    ) -> Option<CryptoFrame> {
        let header_len = HEADER_LEN + NUMBER_LEN;
        let (header, sealed) = datagram.split_at_mut(header_len);
        let plain = key
            .decrypt_in_place(u64::from(number), header, sealed)
            .ok()?
            .len();
        let frame = Self::frame(&datagram[header_len..header_len + plain])?;
        let start = header_len + FRAME_LEN;
        Some(CryptoFrame {
            offset: frame.offset,
            data: datagram.freeze().slice(start..start + frame.len),
        })
    }
}

/// A crypto frame's handshake data, and its offset in its key epoch's stream.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct CryptoFrame {
    pub(crate) offset: u32,
    pub(crate) data: Bytes,
}

/// Where a crypto frame's data starts in its stream, and how long it is.
#[derive(Debug, Clone, Copy)]
struct FrameSpan {
    offset: u32,
    len: usize,
}

fn header(packet_type: PacketType, id: [u8; ID_LEN], capacity: usize) -> BytesMut {
    let mut datagram = BytesMut::with_capacity(capacity);
    datagram.put_u8(packet_type.byte());
    datagram.put_u8(VERSION);
    datagram.put_slice(&id);
    datagram
}

fn put_frame(datagram: &mut BytesMut, offset: u32, data: &[u8]) {
    datagram.put_u32(offset);
    datagram.put_u16(u16::try_from(data.len()).expect("a crypto frame fits a datagram"));
    datagram.put_slice(data);
}

/// An `Initial` of `data` from `offset`, with the client's `token`, padded to `PADDED_INITIAL`
/// when `padded` holds, as a client's are.
pub(crate) fn initial(
    id: [u8; ID_LEN],
    token: Option<&[u8; TOKEN_LEN]>,
    offset: u32,
    data: &[u8],
    padded: Padding,
) -> Bytes {
    debug_assert!(data.len() <= INITIAL_DATA, "an Initial's data fits it");
    let mut datagram = header(PacketType::Initial, id, PADDED_INITIAL);
    match token {
        Some(token) => {
            datagram.put_u8(1);
            datagram.put_slice(token);
        }
        None => datagram.put_u8(0),
    }
    put_frame(&mut datagram, offset, data);
    if padded == Padding::Padded {
        datagram.resize(PADDED_INITIAL, 0);
    }
    datagram.freeze()
}

/// Whether an `Initial` is padded to `PADDED_INITIAL`, as a client's is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Padding {
    Padded,
    Unpadded,
}

pub(crate) fn retry(id: [u8; ID_LEN], token: &[u8; TOKEN_LEN]) -> Bytes {
    let mut datagram = header(PacketType::Retry, id, HEADER_LEN + TOKEN_LEN);
    datagram.put_slice(token);
    datagram.freeze()
}

/// A `Handshake` packet `number` of `data` from `offset`, sealed with `key`.
pub(crate) fn handshake(
    id: [u8; ID_LEN],
    number: u16,
    offset: u32,
    data: &[u8],
    key: &dyn PacketKey,
) -> Bytes {
    debug_assert!(
        data.len() <= HANDSHAKE_DATA,
        "a Handshake packet's data fits it"
    );
    let mut datagram = header(PacketType::Handshake, id, MAX_DATAGRAM);
    datagram.put_u16(number);
    put_frame(&mut datagram, offset, data);
    let header_len = HEADER_LEN + NUMBER_LEN;
    let (header, payload) = datagram.split_at_mut(header_len);
    let tag = key
        .encrypt_in_place(u64::from(number), header, payload)
        .expect("a handshake packet is within its key's limits");
    datagram.put_slice(tag.as_ref());
    datagram.freeze()
}

pub(crate) fn refuse(id: [u8; ID_LEN]) -> Bytes {
    header(PacketType::Refuse, id, HEADER_LEN).freeze()
}

pub(crate) fn alert(id: [u8; ID_LEN], alert: u8) -> Bytes {
    let mut datagram = header(PacketType::Alert, id, HEADER_LEN + 1);
    datagram.put_u8(alert);
    datagram.freeze()
}

#[cfg(test)]
mod tests;
