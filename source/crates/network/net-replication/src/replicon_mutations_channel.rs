use campfire_net_channels::Channel;
/// Replicon's `ServerChannel::Mutations`, unreliable, from the server.
#[derive(Debug)]
pub(crate) struct RepliconMutationsChannel;
impl Channel for RepliconMutationsChannel {}
