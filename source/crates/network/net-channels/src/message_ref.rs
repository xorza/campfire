use crate::fragmented_message::FragmentRef;
use crate::single_body::SingleBody;

/// A channel-held message or fragment, as the packet builder reads it while the channel keeps
/// it.
#[derive(Debug, Clone, Copy)]
pub(crate) enum MessageRef<'a> {
    Single(&'a SingleBody),
    Fragment(FragmentRef<'a>),
}

impl MessageRef<'_> {
    pub(crate) fn bytes_len(self) -> usize {
        match self {
            Self::Single(data) => data.bytes_len(),
            Self::Fragment(fragment) => fragment.bytes_len(),
        }
    }
}
