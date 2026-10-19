use campfire_net_bytes::SerializationError;
use campfire_net_bytes::ToBytes;
use campfire_net_bytes::WriteInteger;
use campfire_net_bytes::varint;
use campfire_net_bytes::{ReadVarInt, Reader};

/// The index of a fragment in a fragmented message.
///
/// It will be serialized as a varint, so it will take only 1 byte if there
/// are less than 64 fragments in the message.
#[derive(Hash, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FragmentIndex(pub(crate) u64);

impl ToBytes for FragmentIndex {
    fn bytes_len(&self) -> usize {
        varint::len(self.0)
    }

    fn to_bytes(&self, buffer: &mut impl WriteInteger) -> Result<(), SerializationError> {
        buffer.write_varint(self.0)?;
        Ok(())
    }

    fn from_bytes(buffer: &mut Reader) -> Result<Self, SerializationError>
    where
        Self: Sized,
    {
        Ok(Self(buffer.read_varint()?))
    }
}
