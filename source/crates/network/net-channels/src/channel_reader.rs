/// Which layer reads a channel's messages off a link (11-net-stack/transport.md, A channel has one
/// reader).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ChannelReader {
    /// The message layer, which reads each message by the type its first bytes name.
    #[default]
    Messages,
    /// The layer that registered the channel, which reads every message on it, as replication
    /// reads replicon's, also on a link it no longer applies them to.
    Owner,
}
