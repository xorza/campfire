use campfire_net_core::Instant;
use core::num::NonZeroU64;

use crate::bandwidth_limit::BandwidthLimit;

/// Token units per byte: a refill of `elapsed_nanos * bytes_per_second` units is exact.
const UNITS_PER_BYTE: u64 = 1_000_000_000;

/// A token bucket of a [`BandwidthLimit`]: it starts full, holds at most the burst, and refills
/// at the sustained rate on the packet session's clock.
#[derive(Debug)]
pub(crate) struct BandwidthLimiter {
    bytes_per_second: u64,
    capacity: u64,
    tokens: u64,
    last_refill: Instant,
}

impl BandwidthLimiter {
    pub(crate) fn new(limit: BandwidthLimit, now: Instant) -> Self {
        let capacity = NonZeroU64::from(limit.burst_bytes).get() * UNITS_PER_BYTE;
        Self {
            bytes_per_second: NonZeroU64::from(limit.bytes_per_second).get(),
            capacity,
            tokens: capacity,
            last_refill: now,
        }
    }

    /// The bytes the bucket holds at `now`.
    pub(crate) fn available(&self, now: Instant) -> usize {
        usize::try_from(self.tokens_at(now) / UNITS_PER_BYTE).unwrap_or(usize::MAX)
    }

    /// Takes `bytes` sent at `now`; a packet past what the bucket holds empties it.
    pub(crate) fn consume(&mut self, bytes: usize, now: Instant) {
        let needed = u64::try_from(bytes)
            .unwrap_or(u64::MAX)
            .saturating_mul(UNITS_PER_BYTE);
        self.tokens = self.tokens_at(now).saturating_sub(needed);
        self.last_refill = self.last_refill.max(now);
    }

    fn tokens_at(&self, now: Instant) -> u64 {
        let elapsed = u64::try_from(now.saturating_duration_since(self.last_refill).as_nanos())
            .unwrap_or(u64::MAX);
        // Saturating is exact here: any sum past `capacity` is clamped to it.
        self.tokens
            .saturating_add(elapsed.saturating_mul(self.bytes_per_second))
            .min(self.capacity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::time::Duration;

    fn at(millis: u64) -> Instant {
        Instant::from_elapsed(Duration::from_millis(millis))
    }

    /// The burst is the limiter's own, whatever the MTU: 300 bytes at once, and nothing after
    /// them.
    #[test]
    fn the_bucket_starts_full_at_its_burst() {
        let mut limiter = BandwidthLimiter::new(BandwidthLimit::new(1).with_burst(300), at(0));
        assert_eq!(limiter.available(at(0)), 300);
        limiter.consume(300, at(0));
        assert_eq!(limiter.available(at(0)), 0);
    }

    /// At 100 B/s with a burst of 300: after 300 bytes at 0 s, 1 s refills 100; half a second
    /// after that, 50 more; and 10 s refill only up to the burst.
    #[test]
    fn the_bucket_refills_at_the_sustained_rate_up_to_the_burst() {
        let mut limiter = BandwidthLimiter::new(BandwidthLimit::new(100).with_burst(300), at(0));
        limiter.consume(300, at(0));
        assert_eq!(limiter.available(at(1_000)), 100);
        limiter.consume(100, at(1_000));
        assert_eq!(limiter.available(at(1_500)), 50);
        assert_eq!(limiter.available(at(11_500)), 300);

        // A rate of 200 B/s refills 200 in the same 1 s, where 100 B/s refilled 100.
        let mut faster = BandwidthLimiter::new(BandwidthLimit::new(200).with_burst(300), at(0));
        faster.consume(300, at(0));
        assert_eq!(faster.available(at(1_000)), 200);
    }

    /// A packet past what the bucket holds empties it, and the refill starts from there.
    #[test]
    fn a_packet_past_the_tokens_empties_the_bucket() {
        let mut limiter = BandwidthLimiter::new(BandwidthLimit::new(100).with_burst(300), at(0));
        limiter.consume(1_000, at(0));
        assert_eq!(limiter.available(at(0)), 0);
        assert_eq!(limiter.available(at(10)), 1);
    }
}
