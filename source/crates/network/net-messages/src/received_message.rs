use crate::message::Message;
use campfire_net_channels::ChannelKind;
use campfire_net_channels::MessageId;
use campfire_net_core::NetTick;

#[derive(Debug)]
pub struct ReceivedMessage<M: Message> {
    pub data: M,
    /// NetTick on the remote peer when the message was sent,
    pub remote_tick: NetTick,
    /// Channel that was used to send the message
    pub channel_kind: ChannelKind,
    /// MessageId of the message
    pub message_id: Option<MessageId>,
}
