use crate::channel_receive_error::ChannelReceiveError;
use crate::packet_error::PacketError;
use campfire_net_bytes::SerializationError;
use campfire_net_core::NetId;

#[derive(thiserror::Error, Debug)]
pub enum TransportError {
    #[error(transparent)]
    SerializationError(#[from] SerializationError),
    #[error(transparent)]
    PacketError(#[from] PacketError),
    #[error("this transport sends on no channel {0:?}")]
    ChannelNotFound(NetId),
    #[error("a message of {size} bytes is above its channel's {max}")]
    MessageTooLarge { size: usize, max: usize },
    #[error("receiver channel error: {0}")]
    ChannelReceiveError(#[from] ChannelReceiveError),
}
