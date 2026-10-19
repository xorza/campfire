use campfire_net_channels::Channel;
/// Marker type for replicon's ServerChannel::Updates (Ordered Reliable, ServerToClient)
#[derive(Debug)]
pub(crate) struct RepliconUpdatesChannel;
impl Channel for RepliconUpdatesChannel {}
