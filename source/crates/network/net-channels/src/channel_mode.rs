/// How a channel's messages are sent and received. See more information
/// [here](https://web.archive.org/web/20250113064633/http://www.jenkinssoftware.com/raknet/manual/reliabilitytypes.html).
///
/// A reliable message goes again each time the session finds its packet lost, until a packet
/// that carries it is delivered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelMode {
    /// Messages may arrive out-of-order, or not at all
    UnorderedUnreliable,
    /// Same as unordered unreliable, but only the newest message is ever accepted, older messages
    /// are ignored
    SequencedUnreliable,
    /// Messages may arrive out-of-order, but each arrives
    UnorderedReliable,
    /// Same as unordered reliable, but the messages are sequenced (only the newest message is
    /// accepted)
    SequencedReliable,
    /// Messages will arrive in the correct order at the destination
    OrderedReliable,
}

impl ChannelMode {
    /// Whether each single message carries its id: a reliable or sequenced channel orders or
    /// deduplicates by it, an unordered unreliable one has no use for it.
    pub(crate) const fn single_has_id(self) -> bool {
        !matches!(self, Self::UnorderedUnreliable)
    }
}
