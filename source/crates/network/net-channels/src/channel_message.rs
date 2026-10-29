use bytes::Bytes;
use campfire_net_core::NetTick;

use crate::message_id::MessageId;

/// A message that a channel delivers, with the tick of the packet that carried it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelMessage {
    pub tick: NetTick,
    pub bytes: Bytes,
    /// The channel's id for the message, on a channel that assigns one.
    pub message_id: Option<MessageId>,
}
