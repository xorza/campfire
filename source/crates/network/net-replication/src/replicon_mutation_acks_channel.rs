use campfire_net_channels::Channel;
/// Marker type for replicon's ClientChannel::MutationAcks (Ordered Reliable, ClientToServer)
#[derive(Debug)]
pub(crate) struct RepliconMutationAcksChannel;
impl Channel for RepliconMutationAcksChannel {}
