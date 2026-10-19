use campfire_net_bytes::SerializationError;
use campfire_net_bytes::ToBytes;
use campfire_net_bytes::VarInt;
use campfire_net_bytes::WriteInteger;
use campfire_net_bytes::{ReadVarInt, Reader};

/// How many fragments a fragmented message has, serialized as a varint.
#[derive(Hash, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FragmentCount(pub(crate) u64);

impl ToBytes for FragmentCount {
    fn bytes_len(&self) -> usize {
        VarInt::new(self.0).encoded_len()
    }

    fn to_bytes(&self, buffer: &mut impl WriteInteger) {
        buffer.write_varint(self.0);
    }

    fn from_bytes(buffer: &mut Reader) -> Result<Self, SerializationError>
    where
        Self: Sized,
    {
        Ok(Self(buffer.read_varint()?))
    }
}
