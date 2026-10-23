use crate::message::Message;
use crate::message_receive_cx::ReceivedMessageHeader;

/// A message as it came: its data, and the header it came with.
#[derive(Debug)]
pub struct ReceivedMessage<M: Message> {
    pub data: M,
    pub header: ReceivedMessageHeader,
}
