use crate::fragment_count::FragmentCount;
use crate::fragment_index::FragmentIndex;
use crate::message_id::MessageId;
use bytes::Bytes;
use campfire_net_bytes::Reader;
use campfire_net_bytes::SerializationError;
use campfire_net_bytes::ToBytes;
use campfire_net_bytes::WriteInteger;
use core::fmt::Debug;

/// One fragment as a packet carries it: its message's id, its index, its message's fragment count
/// and its bytes, which a read slices from the packet's.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FragmentBody {
    pub message_id: MessageId,
    pub fragment_index: FragmentIndex,
    pub fragments: FragmentCount,
    pub bytes: Bytes,
}

impl ToBytes for FragmentBody {
    fn bytes_len(&self) -> usize {
        self.message_id.bytes_len()
            + self.fragment_index.bytes_len()
            + self.fragments.bytes_len()
            + self.bytes.bytes_len()
    }

    fn to_bytes(&self, buffer: &mut impl WriteInteger) {
        self.message_id.to_bytes(buffer);
        self.fragment_index.to_bytes(buffer);
        self.fragments.to_bytes(buffer);
        self.bytes.to_bytes(buffer);
    }

    fn from_bytes(buffer: &mut Reader) -> Result<Self, SerializationError>
    where
        Self: Sized,
    {
        let message_id = MessageId::from_bytes(buffer)?;
        let fragment_index = FragmentIndex::from_bytes(buffer)?;
        let fragments = FragmentCount::from_bytes(buffer)?;
        let bytes = Bytes::from_bytes(buffer)?;
        Ok(Self {
            message_id,
            fragment_index,
            fragments,
            bytes,
        })
    }
}
