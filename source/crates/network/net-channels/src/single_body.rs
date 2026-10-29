use core::fmt::Debug;

use bytes::Bytes;
use campfire_net_bytes::Reader;
use campfire_net_bytes::SerializationError;
use campfire_net_bytes::ToBytes;
use campfire_net_bytes::WriteInteger;

use crate::message_id::MessageId;

#[derive(Clone, Debug, PartialEq)]
/// The serialized bytes of one message: serialized once at send, cloned cheaply after, and sized
/// for fragmentation from the start (11-net-stack/transport.md, A message's id goes with no tag).
///
/// Its id goes with no tag before it: the channel's mode tells whether one follows, and both ends
/// register the same modes.
pub(crate) struct SingleBody {
    pub id: Option<MessageId>,
    pub bytes: Bytes,
}

impl SingleBody {
    pub(crate) const fn new(id: Option<MessageId>, bytes: Bytes) -> Self {
        Self { id, bytes }
    }

    /// The bytes [`write`](Self::write) writes.
    pub(crate) fn bytes_len(&self) -> usize {
        self.id.map_or(0, |id| id.bytes_len()) + self.bytes.bytes_len()
    }

    pub(crate) fn write(&self, buffer: &mut impl WriteInteger) {
        if let Some(id) = self.id {
            id.to_bytes(buffer);
        }
        self.bytes.to_bytes(buffer);
    }

    /// Reads a message of a channel whose messages carry an id when `has_id`.
    pub(crate) fn read(buffer: &mut Reader, has_id: bool) -> Result<Self, SerializationError> {
        let id = if has_id {
            Some(MessageId::from_bytes(buffer)?)
        } else {
            None
        };
        let bytes = Bytes::from_bytes(buffer)?;
        Ok(Self { id, bytes })
    }
}
