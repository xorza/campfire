use campfire_net_channels::Channel;
/// Replicon's `ClientChannel::MutationAcks`, ordered and reliable, from the client.
#[derive(Debug)]
pub(crate) struct RepliconMutationAcksChannel;
impl Channel for RepliconMutationAcksChannel {}
