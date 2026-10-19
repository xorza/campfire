/// The number of a packet in one direction of a packet session, which no other packet of that
/// direction shares: its sender counts from 0, and the wire carries its low 16 bits
/// (13-net-stack/packet-session.md, A packet number goes as its low 16 bits).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PacketNumber(pub u64);
