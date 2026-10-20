use crate::message_id::MessageId;

/// Names a part of a message of a channel: in a flush's candidates and packets, and on a reliable
/// channel in the packets whose fates the transport waits for.
///
/// Every message is a run of parts, and a message that fits one packet is one part, as QUIC
/// names a stream's bytes by their offsets, so a key names no shape the message could contradict
/// (11-net-stack/api.md, A state the code cannot reach is a type). An unreliable channel
/// names a message by its queue index, which holds for one flush, as the channel compacts its
/// queue only once the flush ends; a reliable channel by its id. A fragmented message's candidate
/// names its first due part.
#[derive(Hash, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SendMessageKey {
    pub(crate) message: u32,
    pub(crate) part: u16,
}

impl SendMessageKey {
    pub(crate) const fn new(message: u32, part: u16) -> Self {
        Self { message, part }
    }

    /// The part `part` of a reliable channel's message `id`.
    pub(crate) fn of_id(id: MessageId, part: u16) -> Self {
        Self::new(u32::from(id.0), part)
    }

    /// The reliable message id the key names.
    pub(crate) fn id(self) -> MessageId {
        MessageId(u16::try_from(self.message).expect("a reliable channel's key holds a message id"))
    }

    /// The same message's part `part`.
    pub(crate) const fn with_part(self, part: u16) -> Self {
        Self::new(self.message, part)
    }
}
