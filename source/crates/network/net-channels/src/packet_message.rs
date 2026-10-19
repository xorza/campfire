use crate::bench_counter::BenchCounter;
use crate::send_message_key::SendMessageKey;
use campfire_net_core::NetId;

/// A message that a packet carries, as the sender records it until the packet goes to the
/// session.
///
/// The commit handle updates channel-owned pending data only after the packet goes to the
/// session, and the byte count feeds the channel's post-admission bench counter.
#[derive(Debug, PartialEq)]
pub(crate) struct PacketMessage {
    /// Channel-owned pending state to update after this packet goes to the session.
    pub(crate) commit: SendCommit,
    /// Payload bytes of the message, without its id and length framing (for fragments, only the
    /// fragment's bytes)
    pub(crate) num_bytes: BenchCounter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SendCommit {
    pub(crate) channel_id: NetId,
    pub(crate) key: SendMessageKey,
}
