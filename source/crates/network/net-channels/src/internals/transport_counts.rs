use core::ops::{Add, Sub};

/// A reading of a link's channels' bench counters.
///
/// Every field is accumulated over the channels' life, so a bench subtracts two readings.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TransportCounts {
    /// Bytes of the packets given to the session.
    pub sent_bytes: u64,
    /// Packets given to the session.
    pub sent_packets: u64,
    /// Messages and fragments that a packet given to the session carried again.
    pub resent_messages: u64,
    /// Bytes of the packets the session opened.
    pub received_bytes: u64,
    /// Bytes that compression removed from the packets sent.
    pub compression_saved_bytes: u64,
    /// Packets whose compressed form was not smaller, so they were sent uncompressed.
    pub compression_abandoned: u64,
}

impl Add for TransportCounts {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self {
            sent_bytes: self.sent_bytes + rhs.sent_bytes,
            sent_packets: self.sent_packets + rhs.sent_packets,
            resent_messages: self.resent_messages + rhs.resent_messages,
            received_bytes: self.received_bytes + rhs.received_bytes,
            compression_saved_bytes: self.compression_saved_bytes + rhs.compression_saved_bytes,
            compression_abandoned: self.compression_abandoned + rhs.compression_abandoned,
        }
    }
}

impl Sub for TransportCounts {
    type Output = Self;

    fn sub(self, base: Self) -> Self {
        Self {
            sent_bytes: self.sent_bytes - base.sent_bytes,
            sent_packets: self.sent_packets - base.sent_packets,
            resent_messages: self.resent_messages - base.resent_messages,
            received_bytes: self.received_bytes - base.received_bytes,
            compression_saved_bytes: self.compression_saved_bytes - base.compression_saved_bytes,
            compression_abandoned: self.compression_abandoned - base.compression_abandoned,
        }
    }
}
