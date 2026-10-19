use crate::message::Message;
use campfire_net_channels::ChannelTypeId;
use campfire_net_channels::MessageId;
use campfire_net_core::NetTick;

#[derive(Debug)]
pub struct ReceivedMessage<M: Message> {
    pub data: M,
    /// NetTick on the remote peer when the message was sent,
    pub remote_tick: NetTick,
    /// The channel the message came on.
    pub channel_type_id: ChannelTypeId,
    pub message_id: Option<MessageId>,
}
