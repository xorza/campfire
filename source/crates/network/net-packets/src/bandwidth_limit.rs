use core::num::NonZeroU32;

/// The rate an app lets each session send its payloads at: a sustained rate, and a burst after
/// the session was quiet. The session's budget is the smaller of its congestion window's room and
/// this limit's (13-net-stack/session.md, Congestion control is NewReno, in the session).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BandwidthLimit {
    pub bytes_per_second: NonZeroU32,
    pub burst_bytes: NonZeroU32,
}

impl BandwidthLimit {
    /// A sustained rate of `bytes_per_second`, with a burst of as many bytes.
    pub const fn new(bytes_per_second: u32) -> Self {
        let rate = NonZeroU32::new(bytes_per_second).expect("the byte rate is not zero");
        Self {
            bytes_per_second: rate,
            burst_bytes: rate,
        }
    }

    /// The bytes a quiet session sends at once.
    #[must_use]
    pub const fn with_burst(mut self, burst_bytes: u32) -> Self {
        self.burst_bytes = NonZeroU32::new(burst_bytes).expect("the burst is not zero");
        self
    }
}
