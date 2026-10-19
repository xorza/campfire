/// What a [`Channels`](crate::channels::Channels) still waits for.
///
/// Once a healed network has delivered everything owed, no fragmented message is partial and only
/// the last frame's packets wait for their fates.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TransportBacklog {
    /// Fragmented messages with some fragments still missing, over every channel.
    pub partial_fragments: usize,
    /// Packets whose reliable messages wait for the packet's fate.
    pub tracked_packet_acks: usize,
}
