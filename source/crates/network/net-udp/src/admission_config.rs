use core::num::NonZeroU32;
use core::time::Duration;

/// What a server admits of each address (13-net-stack/udp.md, Each address holds a bounded count of
/// connections and handshakes): at most `per_address` live connections from one IPv4 address or
/// IPv6 /64, and `per_address` new handshakes from it, which come back over `refill`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdmissionConfig {
    per_address: NonZeroU32,
    refill: Duration,
}

impl AdmissionConfig {
    pub const fn new(per_address: NonZeroU32, refill: Duration) -> Self {
        Self {
            per_address,
            refill,
        }
    }

    pub(crate) const fn per_address(self) -> NonZeroU32 {
        self.per_address
    }

    pub(crate) const fn refill(self) -> Duration {
        self.refill
    }
}
