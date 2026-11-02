/// Whether a channel's single messages carry their id on the wire: a reliable or sequenced
/// channel orders or deduplicates by it, an unordered unreliable one has no use for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MessageIdTag {
    Tagged,
    Untagged,
}
