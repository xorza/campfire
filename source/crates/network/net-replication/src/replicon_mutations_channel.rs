use campfire_net_channels::Channel;
/// Marker type for replicon's ServerChannel::Mutations (Unreliable, ServerToClient)
#[derive(Debug)]
pub(crate) struct RepliconMutationsChannel;
impl Channel for RepliconMutationsChannel {}
