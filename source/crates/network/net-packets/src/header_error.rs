/// Why a datagram is not a session packet the receiver reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum HeaderError {
    #[error("the datagram is shorter than its header")]
    Truncated,
    #[error("the datagram's first bit names its IO's own packet")]
    IoPacket,
    #[error("the packet type {0} is unknown")]
    UnknownType(u8),
    #[error("the header sets a reserved bit")]
    Reserved,
    #[error(
        "a packet of its type carries a payload kind, or lacks or holds acknowledgements against its type"
    )]
    Shape,
}
