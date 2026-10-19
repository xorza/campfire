#[cfg(feature = "reflect")]
use bevy_reflect::Reflect;
use campfire_net_bytes::SerializationError;
use campfire_net_bytes::ToBytes;
use campfire_net_bytes::WriteInteger;
use campfire_net_bytes::varint;
use campfire_net_bytes::{ReadVarInt, Reader};

/// The id a protocol registry gives each registered type, in registration order: what names a
/// message, a component or a channel on the wire. A varint of the `u16`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "reflect", derive(Reflect))]
pub struct NetId(u16);

impl NetId {
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u16 {
        self.0
    }
}

impl ToBytes for NetId {
    fn bytes_len(&self) -> usize {
        varint::len(u64::from(self.0))
    }

    fn to_bytes(&self, buffer: &mut impl WriteInteger) {
        buffer.write_varint(u64::from(self.0));
    }

    fn from_bytes(buffer: &mut Reader) -> Result<Self, SerializationError> {
        u16::try_from(buffer.read_varint()?)
            .map(Self)
            .map_err(|_| SerializationError::InvalidValue)
    }
}
