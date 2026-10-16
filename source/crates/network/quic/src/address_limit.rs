use std::num::NonZeroU32;
use std::time::Duration;

/// The limit of a server's endpoint for each address (15-quic-transport.md, Limits for each
/// address, from the session's seats): at most `seats` live connections from one IPv4 address or
/// IPv6 /64, and `seats` new handshakes from it, which come back over `refill`, the session's grace
/// period. A server's endpoint serves with one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AddressLimit {
    seats: NonZeroU32,
    refill: Duration,
}

impl AddressLimit {
    pub const fn new(seats: NonZeroU32, refill: Duration) -> Self {
        Self { seats, refill }
    }

    pub(crate) const fn seats(self) -> NonZeroU32 {
        self.seats
    }

    pub(crate) const fn refill(self) -> Duration {
        self.refill
    }
}
