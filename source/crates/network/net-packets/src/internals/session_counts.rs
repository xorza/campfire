use crate::header::PacketType;

/// The packets of its own that a session sent, which a bench reads: each accumulates over the
/// session's life.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SessionCounts {
    /// `Ping`s: the first packet, the keep-alives and the probes.
    pub pings: u64,
    /// Packets that carry only an ACK.
    pub ack_only: u64,
}

impl SessionCounts {
    pub(crate) const fn count(&mut self, packet_type: PacketType) {
        match packet_type {
            PacketType::Ping => self.pings += 1,
            PacketType::AckOnly => self.ack_only += 1,
            PacketType::Payload
            | PacketType::PathChallenge
            | PacketType::PathResponse
            | PacketType::Close => {}
        }
    }
}
