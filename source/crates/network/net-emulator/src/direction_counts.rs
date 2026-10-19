/// What one direction of an emulated link did with its packets so far.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DirectionCounts {
    /// Packets put on the direction.
    pub sent: u64,
    /// Bytes of the packets put on the direction.
    pub sent_bytes: u64,
    /// Packets lost, dropped at the MTU or the rate limit's queue, or held by a stall that never
    /// ends.
    pub dropped: u64,
    /// Copies the model added.
    pub duplicated: u64,
    /// Packets handed to the receiving end, copies included.
    pub delivered: u64,
}
