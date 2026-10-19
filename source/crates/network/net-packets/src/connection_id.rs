use crate::header::IO_BIT;

/// The ID a server gives a session, which starts each of the client's datagrams and routes it: 63
/// random bits, as the first bit is the IO's. No two sessions under one reset key share an ID, so
/// no reset token an attacker draws from the server names a later session (13-net-stack/udp.md, A
/// client packet starts with the server's random connection ID).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConnectionId([u8; Self::LEN]);

impl ConnectionId {
    /// The bytes of an ID on the wire.
    pub const LEN: usize = 8;

    /// The ID of `bytes`, drawn at random, with its first bit cleared for the IO.
    pub const fn from_random(mut bytes: [u8; Self::LEN]) -> Self {
        bytes[0] &= !IO_BIT;
        Self(bytes)
    }

    /// The ID `bytes` hold; `None` when its first bit is set, as an IO packet's is.
    pub const fn from_bytes(bytes: [u8; Self::LEN]) -> Option<Self> {
        if bytes[0] & IO_BIT == 0 {
            Some(Self(bytes))
        } else {
            None
        }
    }

    pub const fn to_bytes(self) -> [u8; Self::LEN] {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A random ID loses its first bit and keeps the rest; read bytes with the first bit set are
    /// no ID.
    #[test]
    fn an_id_leaves_the_first_bit_to_the_io() {
        let id = ConnectionId::from_random([0xff; ConnectionId::LEN]);
        assert_eq!(
            id.to_bytes(),
            [0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]
        );
        assert_eq!(ConnectionId::from_bytes(id.to_bytes()), Some(id));
        assert_eq!(ConnectionId::from_bytes([0x80; ConnectionId::LEN]), None);
    }
}
