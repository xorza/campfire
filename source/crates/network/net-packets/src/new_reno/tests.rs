use core::time::Duration;

use super::*;

impl NewReno {
    pub(crate) const fn window(&self) -> usize {
        self.window
    }

    pub(crate) const fn bytes_in_flight(&self) -> usize {
        self.bytes_in_flight
    }
}

fn at(millis: u64) -> Instant {
    Instant::from_elapsed(Duration::from_millis(millis))
}

/// With the window full, an acknowledgement of 1,200 bytes grows it by 1,200 in slow start:
/// 12,000 to 13,200. A loss halves it to 6,600 and sets the threshold there; an
/// acknowledgement of a packet sent before the loss grows nothing, and one sent after, in
/// congestion avoidance, grows it by nothing until a window's bytes were acknowledged.
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
    assert_eq!(reno.window(), 6_600);
    assert_eq!(reno.bytes_in_flight(), 6 * MAX_DATAGRAM);
}

/// In congestion avoidance at 6,000 bytes, with the window kept full of 200-byte packets, 29
/// acknowledgements, 5,800 bytes, grow nothing, and the 30th, 6,000, grows it by a datagram, to
/// 7,200; the next datagram takes 7,200 bytes more. Each acknowledgement's 1,200 · 200 / 6,000
/// = 40 bytes would grow it the same here, but 36.4, at 6,600, truncates to 36.
#[test]
fn the_window_grows_a_datagram_for_each_window_acknowledged() {
    let mut reno = NewReno::default();
    reno.on_sent(MAX_DATAGRAM);
    reno.on_lost(MAX_DATAGRAM, at(1), at(10));
    assert_eq!(reno.window(), 6_000);
    let mut windows = Vec::new();
    for _ in 0..66 {
        while reno.room() > 0 {
            reno.on_sent(200);
        }
        reno.on_acked(200, at(11));
        windows.push(reno.window());
    }
    assert_eq!(windows[28], 6_000);
    assert_eq!(windows[29], 7_200);
    // 7,200 more bytes: 36 acknowledgements after the 30th.
    assert_eq!(windows[64], 7_200);
    assert_eq!(windows[65], 8_400);
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
    assert_eq!(reno.window(), MIN_WINDOW);
    reno.on_persistent_congestion();
    assert_eq!(reno.window(), MIN_WINDOW);
}

/// An app-limited sender, with more than a datagram of room left, does not grow the window.
#[test]
fn an_app_limited_sender_does_not_grow_the_window() {
    let mut reno = NewReno::default();
    reno.on_sent(MAX_DATAGRAM);
    reno.on_acked(MAX_DATAGRAM, at(0));
    assert_eq!(reno.window(), INITIAL_WINDOW);
}
