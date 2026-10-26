use campfire_net_channels::Channel;
/// The reliable channel of each [`SenderHello`](crate::sender_hello::SenderHello).
#[derive(Debug)]
pub(crate) struct HelloChannel;
impl Channel for HelloChannel {}
