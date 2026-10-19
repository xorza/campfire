use campfire_net_bytes::SerializationError;
use campfire_net_channels::TransportError;
use campfire_net_core::NetId;

/// Why a message could not be sent or received: bad data, a peer's protocol violation, or a
/// message above its channel's size limit.
#[derive(thiserror::Error, Debug)]
pub enum MessageError {
    #[error(transparent)]
    Serialization(#[from] SerializationError),
    #[error("the message net id {0:?} is not registered")]
    UnrecognizedNetId(NetId),
    #[error("the message net id {0:?} arrived against its registered direction")]
    WrongDirection(NetId),
    #[error(transparent)]
    Transport(#[from] TransportError),
}
