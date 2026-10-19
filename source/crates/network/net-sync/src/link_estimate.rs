use core::time::Duration;

/// A link's latency estimate once its handshake completes: what timeline synchronization reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkEstimate {
    rtt: Duration,
    jitter: Duration,
}

impl LinkEstimate {
    pub const fn new(rtt: Duration, jitter: Duration) -> Self {
        Self { rtt, jitter }
    }

    /// The smoothed round-trip time.
    pub const fn rtt(&self) -> Duration {
        self.rtt
    }

    /// The variation in packet delay.
    pub const fn jitter(&self) -> Duration {
        self.jitter
    }
}
