use crate::compression_config::CompressionConfig;
use crate::message_data::MessageData;
use campfire_net_core::NetTick;
use core::fmt::Debug;

#[derive(Debug, PartialEq)]
pub(crate) struct ReceiveMessage {
    pub(crate) data: MessageData,
    // keep track on the receiver side of the sender tick when the message was actually sent
    pub(crate) remote_sent_tick: NetTick,
    pub(crate) compression: CompressionConfig,
}
