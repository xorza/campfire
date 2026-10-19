use crate::message_id::MessageId;

/// Names a message or fragment of a channel: in a flush's candidates and packets, and on a
/// reliable channel in the packets whose fates the transport waits for.
///
/// An unreliable channel names a message by its queue index, which holds for one flush, as the
/// channel compacts its queues only once the flush ends; a reliable channel by its id. A
/// fragmented message's candidate names its first due fragment.
#[derive(Hash, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SendMessageKey {
    UnreliableSingle(u32),
    UnreliableFragment(u32, u16),
    ReliableSingle(MessageId),
    ReliableFragment(MessageId, u16),
}

impl SendMessageKey {
    pub(crate) const fn is_single(self) -> bool {
        matches!(self, Self::UnreliableSingle(_) | Self::ReliableSingle(_))
    }

    /// The same message's fragment `index`, for a key of a fragment.
    pub(crate) const fn with_fragment(self, index: u16) -> Self {
        match self {
            Self::UnreliableFragment(slot, _) => Self::UnreliableFragment(slot, index),
            Self::ReliableFragment(id, _) => Self::ReliableFragment(id, index),
            Self::UnreliableSingle(_) | Self::ReliableSingle(_) => {
                panic!("a single message has no fragments")
            }
        }
    }
}
