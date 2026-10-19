//! Errors for building packets

use crate::channel_receive_error::ChannelReceiveError;
use campfire_net_bytes::SerializationError;

#[derive(thiserror::Error, Debug)]
pub enum PacketError {
    #[error("serialization error: {0}")]
    Serialization(#[from] SerializationError),
    #[error("channel was not found")]
    ChannelNotFound,
    #[error("unknown packet kind {0}")]
    UnknownPacketKind(u8),
    #[error("a packet's tick is short before any whole one, or its long form is malformed")]
    MalformedTick,
    #[error("receiver channel error: {0}")]
    ChannelReceiveError(#[from] ChannelReceiveError),
    #[error("compression is not supported by this build or transport configuration")]
    UnsupportedCompression,
    #[error("packet payload could not be compressed")]
    CompressionFailed,
    #[error("compressed packet payload could not be decompressed")]
    DecompressionFailed,
    #[error("decompressed packet payload size {actual} exceeds configured limit {limit}")]
    DecompressedPayloadTooLarge { actual: usize, limit: usize },
    #[error("packet size {actual} exceeds the largest payload {max}")]
    PacketTooLarge { actual: usize, max: usize },
    #[error("the largest payload {actual} is too small for fragment packets; minimum is {min}")]
    PayloadTooSmall { actual: usize, min: usize },
}
