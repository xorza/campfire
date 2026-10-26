use campfire_net_bytes::SerializationError;
use campfire_net_core::NetId;

/// A claim of a received packet that no honest peer of the same protocol makes: its link ends as a
/// protocol violation (11-net-stack/transport.md, A received packet ends one of three ways).
#[derive(thiserror::Error, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiveError {
    #[error("the bytes do not decode: {0}")]
    Serialization(#[from] SerializationError),
    #[error("payload kind {0} is no packet kind")]
    UnknownPacketKind(u8),
    #[error("a packet's tick is short before any whole one, or its long form is malformed")]
    MalformedTick,
    #[error("the packet names channel {0:?}, which this link does not receive on")]
    UnknownChannel(NetId),
    #[error("a batch holds no message")]
    EmptyBatch,
    /// A reliable message's id lies as far as the sender's window or farther ahead of the id this
    /// end waits for, where the sender keeps every id it sends.
    #[error("a reliable message's id lies past the sender's window")]
    PastWindow,
    #[error("fragmented message declared an invalid fragment count: {fragments}")]
    InvalidFragmentCount { fragments: u64 },
    #[error("fragment index {fragment_index} is outside fragment count {fragments}")]
    InvalidFragmentIndex { fragment_index: u64, fragments: u64 },
    #[error("fragment count changed while reassembling message: expected {expected}, got {actual}")]
    FragmentCountMismatch { expected: usize, actual: usize },
    #[error("{fragments} fragments can not fit a message of at most {max_message_len} bytes")]
    FragmentCountTooLarge {
        fragments: u64,
        max_message_len: usize,
    },
    #[error(
        "fragments describe a message of at least {min_len} bytes, above the channel's {max_message_len}"
    )]
    MessageTooLarge {
        min_len: usize,
        max_message_len: usize,
    },
    #[error("a fragment is empty")]
    EmptyFragment,
    #[error("non-final fragment has size {actual}, expected {expected}")]
    InvalidNonFinalFragmentLen { actual: usize, expected: usize },
    #[error("final fragment has size {actual}, maximum {max}")]
    InvalidFinalFragmentLen { actual: usize, max: usize },
}

/// Why a send was refused; nothing is staged then. A reliable channel's refusal ends its link
/// (11-net-stack/transport.md, A link's queued bytes have a budget).
#[derive(thiserror::Error, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendRefusal {
    #[error("a message of {len} bytes is above its channel's {max_len}")]
    MessageTooLarge { len: usize, max_len: usize },
    /// The link's queued bytes would pass its send budget: its peer acknowledges less than it is
    /// sent.
    #[error("a message of {len} bytes passes the link's send budget, with {queued} queued")]
    Backlogged { len: usize, queued: usize },
}
