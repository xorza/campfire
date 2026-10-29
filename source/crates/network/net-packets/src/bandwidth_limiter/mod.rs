use core::num::NonZeroU64;

use campfire_net_core::Instant;

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
mod tests;
