//! The congestion window, as RFC 9002 §7 keeps it with NewReno.

use campfire_net_core::Instant;
use campfire_net_payload::DEFAULT_MTU;

/// The largest datagram a session sends: the fixed MTU of 1,200 bytes (RFC 9000 §14).
pub(crate) const MAX_DATAGRAM: usize = DEFAULT_MTU;

/// The window a session starts with: `min(10 · MAX_DATAGRAM, max(14,720, 2 · MAX_DATAGRAM))`,
/// 12,000 bytes (RFC 9002 §7.2).
const INITIAL_WINDOW: usize = 10 * MAX_DATAGRAM;

/// The window never shrinks below two datagrams (RFC 9002 §7.2, `kMinimumWindow`).
pub(crate) const MINIMUM_WINDOW: usize = 2 * MAX_DATAGRAM;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NewReno {
    window: usize,
    slow_start_threshold: usize,
    bytes_in_flight: usize,
    /// When the last congestion event began: a loss of a packet sent before it starts no other.
    recovery_start: Option<Instant>,
}

impl Default for NewReno {
    fn default() -> Self {
        Self {
            window: INITIAL_WINDOW,
            slow_start_threshold: usize::MAX,
            bytes_in_flight: 0,
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
    /// the bytes in slow start, and by `MAX_DATAGRAM · bytes / window` in congestion avoidance;
    /// not for a packet sent in the current recovery, nor while the sender left a datagram's
    /// room unused, as an app-limited sender does (RFC 9002 §7.8).
    pub(crate) fn on_acked(&mut self, bytes: usize, sent_at: Instant) {
        let limited = self.bytes_in_flight + MAX_DATAGRAM > self.window;
        self.bytes_in_flight -= bytes;
        if self.in_recovery(sent_at) || !limited {
            return;
        }
        self.window += if self.window < self.slow_start_threshold {
            bytes
        } else {
            MAX_DATAGRAM * bytes / self.window
        };
    }

    /// A packet of `bytes` in flight, sent at `sent_at`, was lost at `now`. A loss of a packet
    /// sent after the current recovery began starts a new one: the threshold and the window halve,
    /// the window to no less than `MINIMUM_WINDOW`.
    pub(crate) fn on_lost(&mut self, bytes: usize, sent_at: Instant, now: Instant) {
        self.bytes_in_flight -= bytes;
        if self.in_recovery(sent_at) {
            return;
        }
        self.recovery_start = Some(now);
        self.slow_start_threshold = self.window / 2;
        self.window = self.slow_start_threshold.max(MINIMUM_WINDOW);
    }

    /// Losses spanned the persistent congestion duration with no acknowledgement among them: the
    /// window falls to `MINIMUM_WINDOW` (RFC 9002 §7.6.2).
    pub(crate) const fn on_persistent_congestion(&mut self) {
        self.window = MINIMUM_WINDOW;
        self.recovery_start = None;
    }

    fn in_recovery(&self, sent_at: Instant) -> bool {
        self.recovery_start.is_some_and(|start| sent_at <= start)
    }
}

#[cfg(test)]
impl NewReno {
    pub(crate) const fn window(&self) -> usize {
        self.window
    }

    pub(crate) const fn bytes_in_flight(&self) -> usize {
        self.bytes_in_flight
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::time::Duration;

    fn at(millis: u64) -> Instant {
        Instant::from_elapsed(Duration::from_millis(millis))
    }

    /// With the window full, an acknowledgement of 1,200 bytes grows it by 1,200 in slow start:
    /// 12,000 to 13,200. A loss halves it to 6,600 and sets the threshold there; an
    /// acknowledgement of a packet sent before the loss grows nothing, and one sent after grows it
    /// by 1,200 · 1,200 / 6,600 = 218 bytes, in congestion avoidance.
    #[test]
    fn the_window_grows_and_halves_by_newreno() {
        let mut reno = NewReno::default();
        for _ in 0..10 {
            reno.on_sent(MAX_DATAGRAM);
        }
        assert_eq!(reno.room(), 0);
        reno.on_acked(MAX_DATAGRAM, at(0));
        assert_eq!(reno.window(), 13_200);
        reno.on_lost(MAX_DATAGRAM, at(1), at(10));
        assert_eq!(reno.window(), 6_600);
        // 8 · 1,200 = 9,600 bytes in flight against 6,600: limited.
        reno.on_acked(MAX_DATAGRAM, at(5));
        assert_eq!(reno.window(), 6_600);
        reno.on_acked(MAX_DATAGRAM, at(11));
        assert_eq!(reno.window(), 6_600 + 218);
        assert_eq!(reno.bytes_in_flight(), 6 * MAX_DATAGRAM);
    }

    /// A loss of a packet sent before the recovery began starts no other, and the window does not
    /// fall below two datagrams.
    #[test]
    fn one_recovery_halves_once_and_the_window_has_a_floor() {
        let mut reno = NewReno::default();
        for _ in 0..4 {
            reno.on_sent(MAX_DATAGRAM);
        }
        reno.on_lost(MAX_DATAGRAM, at(1), at(10));
        reno.on_lost(MAX_DATAGRAM, at(2), at(11));
        assert_eq!(reno.window(), 6_000);
        reno.on_lost(MAX_DATAGRAM, at(12), at(20));
        reno.on_lost(MAX_DATAGRAM, at(21), at(30));
        assert_eq!(reno.window(), MINIMUM_WINDOW);
        reno.on_persistent_congestion();
        assert_eq!(reno.window(), MINIMUM_WINDOW);
    }

    /// An app-limited sender, with more than a datagram of room left, does not grow the window.
    #[test]
    fn an_app_limited_sender_does_not_grow_the_window() {
        let mut reno = NewReno::default();
        reno.on_sent(MAX_DATAGRAM);
        reno.on_acked(MAX_DATAGRAM, at(0));
        assert_eq!(reno.window(), INITIAL_WINDOW);
    }
}
