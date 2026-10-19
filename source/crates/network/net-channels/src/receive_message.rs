use crate::compression_setup::CompressionSetup;
use crate::message_body::MessageBody;
use campfire_net_core::NetTick;
use core::fmt::Debug;

#[derive(Debug, PartialEq)]
pub(crate) struct ReceiveMessage {
    pub(crate) data: MessageBody,
    // keep track on the receiver side of the sender tick when the message was actually sent
    pub(crate) remote_sent_tick: NetTick,
    pub(crate) compression: CompressionSetup,
}
