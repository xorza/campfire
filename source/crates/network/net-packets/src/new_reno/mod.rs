//! The congestion window, as RFC 9002 §7 keeps it with NewReno.

use campfire_net_core::Instant;
use campfire_net_payload::DEFAULT_MTU;

/// The largest datagram a packet session sends: the fixed MTU of 1,200 bytes (RFC 9000 §14).
pub(crate) const MAX_DATAGRAM: usize = DEFAULT_MTU;

/// The window a packet session starts with: `min(10 · MAX_DATAGRAM, max(14,720, 2 ·
/// MAX_DATAGRAM))`, 12,000 bytes (RFC 9002 §7.2).
const INITIAL_WINDOW: usize = 10 * MAX_DATAGRAM;

/// The window never shrinks below two datagrams (RFC 9002 §7.2, `kMinimumWindow`).
pub(crate) const MIN_WINDOW: usize = 2 * MAX_DATAGRAM;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NewReno {
    window: usize,
    slow_start_threshold: usize,
    bytes_in_flight: usize,
    /// The bytes acknowledged in congestion avoidance since the window last grew there.
    acked: usize,
    /// When the last congestion event began: a loss of a packet sent before it starts no other.
    recovery_start: Option<Instant>,
}

impl Default for NewReno {
    fn default() -> Self {
        Self {
            window: INITIAL_WINDOW,
            slow_start_threshold: usize::MAX,
            bytes_in_flight: 0,
            acked: 0,
            recovery_start: None,
        }
    }
}

impl NewReno {
    /// The bytes the window leaves above the bytes in flight.
    pub(crate) const fn room(&self) -> usize {
        self.window.saturating_sub(self.bytes_in_flight)
    }

    /// A packet of `bytes` that counts in flight was sent.
    pub(crate) const fn on_sent(&mut self, bytes: usize) {
        self.bytes_in_flight += bytes;
    }

    /// A packet of `bytes` in flight, sent at `sent_at`, was acknowledged. The window grows by
    /// the bytes in slow start, and in congestion avoidance by `MAX_DATAGRAM` for each window's
    /// bytes acknowledged, counted whole, so small packets grow it as large ones do (RFC 9002
    /// §7.3.3; quinn's NewReno), where `MAX_DATAGRAM · bytes / window` for each acknowledgement
    /// truncates; not for a packet sent in the current recovery, nor while the sender left a
    /// datagram's room unused, as an app-limited sender does (RFC 9002 §7.8).
    pub(crate) fn on_acked(&mut self, bytes: usize, sent_at: Instant) {
        let limited = self.bytes_in_flight + MAX_DATAGRAM > self.window;
        self.bytes_in_flight -= bytes;
        if self.is_in_recovery(sent_at) || !limited {
            return;
        }
        if self.window < self.slow_start_threshold {
            self.window += bytes;
            return;
        }
        self.acked += bytes;
        if self.acked >= self.window {
            self.acked -= self.window;
            self.window += MAX_DATAGRAM;
        }
    }

    /// A packet of `bytes` in flight, sent at `sent_at`, was lost at `now`. A loss of a packet
    /// sent after the current recovery began starts a new one: the threshold and the window halve,
    /// the window to no less than `MIN_WINDOW`.
    pub(crate) fn on_lost(&mut self, bytes: usize, sent_at: Instant, now: Instant) {
        self.bytes_in_flight -= bytes;
        if self.is_in_recovery(sent_at) {
            return;
        }
        self.recovery_start = Some(now);
        self.slow_start_threshold = self.window / 2;
        self.window = self.slow_start_threshold.max(MIN_WINDOW);
        self.acked = 0;
    }

    /// Losses spanned the persistent congestion duration with no acknowledgement among them: the
    /// window falls to `MIN_WINDOW` (RFC 9002 §7.6.2).
    pub(crate) const fn on_persistent_congestion(&mut self) {
        self.window = MIN_WINDOW;
        self.acked = 0;
        self.recovery_start = None;
    }

    fn is_in_recovery(&self, sent_at: Instant) -> bool {
        self.recovery_start.is_some_and(|start| sent_at <= start)
    }
}

#[cfg(test)]
mod tests;
