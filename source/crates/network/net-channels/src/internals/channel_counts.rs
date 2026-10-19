use core::ops::{Add, Sub};

/// A reading of one channel's bench counters on a [`Channels`](crate::channels::Channels).
///
/// The byte fields count message payload bytes (for a fragment, the fragment's bytes); every field
/// accumulates over the channel's life, so a bench subtracts two readings.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ChannelCounts {
    /// Bytes of the channel's messages in packets given to the session.
    pub sent_bytes: u64,
    /// The channel's messages and fragments in packets given to the session.
    pub sent_messages: u64,
    /// Bytes of the channel's messages parsed from received packets.
    pub received_bytes: u64,
}

impl Add for ChannelCounts {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self {
            sent_bytes: self.sent_bytes + rhs.sent_bytes,
            sent_messages: self.sent_messages + rhs.sent_messages,
            received_bytes: self.received_bytes + rhs.received_bytes,
        }
    }
}

impl Sub for ChannelCounts {
    type Output = Self;

    fn sub(self, base: Self) -> Self {
        Self {
            sent_bytes: self.sent_bytes - base.sent_bytes,
            sent_messages: self.sent_messages - base.sent_messages,
            received_bytes: self.received_bytes - base.received_bytes,
        }
    }
}
