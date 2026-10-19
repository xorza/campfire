use campfire_net_channels::Channel;
/// Replicon's `ServerChannel::Updates`, ordered and reliable, from the server.
#[derive(Debug)]
pub(crate) struct RepliconUpdatesChannel;
impl Channel for RepliconUpdatesChannel {}
