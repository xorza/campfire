use crate::fragment_compression::FragmentCompression;
use crate::fragment_count::FragmentCount;
use crate::fragment_index::FragmentIndex;
use crate::message_id::MessageId;
use bytes::Bytes;
use campfire_net_bytes::Reader;
use campfire_net_bytes::SerializationError;
use campfire_net_bytes::ToBytes;
use campfire_net_bytes::WriteInteger;
use core::fmt::Debug;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FragmentData {
    // we always need a message_id for fragment messages, for re-assembly
    pub message_id: MessageId,
    pub fragment_index: FragmentIndex,
    pub num_fragments: FragmentCount,
    /// Compression mode for the reassembled message. Serialized only on fragment 0.
    pub compression: Option<FragmentCompression>,
    /// Bytes data associated with the message that is too big
    pub bytes: Bytes,
}

impl ToBytes for FragmentData {
    fn bytes_len(&self) -> usize {
        self.message_id.bytes_len()
            + self.fragment_index.bytes_len()
            + self.num_fragments.bytes_len()
            + if self.is_initial_fragment() {
                FragmentCompression::None.bytes_len()
            } else {
                0
            }
            + self.bytes.bytes_len()
    }

    fn to_bytes(&self, buffer: &mut impl WriteInteger) {
        self.message_id.to_bytes(buffer);
        self.fragment_index.to_bytes(buffer);
        self.num_fragments.to_bytes(buffer);
        if self.is_initial_fragment() {
            self.compression.unwrap_or_default().to_bytes(buffer);
        }
        self.bytes.to_bytes(buffer);
    }

    /// We get the FragmentData as a subslice of the original Bytes. O(1) operation.
    fn from_bytes(buffer: &mut Reader) -> Result<Self, SerializationError>
    where
        Self: Sized,
    {
        let message_id = MessageId::from_bytes(buffer)?;
        let fragment_index = FragmentIndex::from_bytes(buffer)?;
        let num_fragments = FragmentCount::from_bytes(buffer)?;
        let compression = if fragment_index.0 == 0 {
            Some(FragmentCompression::from_bytes(buffer)?)
        } else {
            None
        };
        let bytes = Bytes::from_bytes(buffer)?;
        Ok(Self {
            message_id,
            fragment_index,
            num_fragments,
            compression,
            bytes,
        })
    }
}

impl FragmentData {
    pub(crate) const fn is_initial_fragment(&self) -> bool {
        self.fragment_index.0 == 0
    }
}
