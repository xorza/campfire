use campfire_codec::BinaryError;
use campfire_net_bytes::SerializationError;
use campfire_net_core::NetId;

/// Why a received message could not be read: bad data, or a peer's protocol violation.
#[derive(thiserror::Error, Debug)]
pub enum MessageError {
    #[error(transparent)]
    Serialization(#[from] SerializationError),
    /// A message that `codec` does not read, or whose bytes are not its one encoding.
    #[error(transparent)]
    Binary(#[from] BinaryError),
    #[error("the message net id {0:?} is not registered")]
    UnrecognizedNetId(NetId),
    #[error("the message net id {0:?} arrived against its registered direction")]
    WrongDirection(NetId),
}
