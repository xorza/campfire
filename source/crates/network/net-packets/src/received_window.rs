//! The packets of the peer's direction that this end opened and accepted. One window of 64
//! numbers is the ACK bits the header carries below the largest accepted, the replay window, and
//! the payloads a frame holds below the largest opened, so one ACK names every packet one frame
//! accepts: a packet at or before the window's start is dropped, as its replay cannot be told
//! (RFC 4303 §3.4.3).
use core::time::Duration;

use campfire_net_core::Instant;
use campfire_net_payload::PacketNumber;

use crate::header::AckFields;
use crate::packet_numbers;

/// The numbers below the largest accepted that the ACK bits name, and those at and below the
/// largest opened that the pending bits name: a payload further below it is dropped, as is one
/// that leaves them while it waits, so a packet session holds at most this many payloads for its
/// frame, and every one an ACK names (11-net-stack/packet-session.md, One window of 64 packets is
/// the ACK bits, the replay window and a frame's payloads).
pub(crate) const WINDOW: u64 = 64;

/// The largest time an accepted packet that needs an ACK waits for one, which every link's peer
/// assumes, so no link negotiates it (RFC 9000 §13.2.1) (11-net-stack/packet-session.md, An
/// ACK-only packet goes when an accepted packet waited 25 ms and no packet carried its ACK).
pub(crate) const MAX_ACK_DELAY: Duration = Duration::from_millis(25);

/// The packets a packet session accepted, which its ACKs name and which drop a copy or a replay of
/// one (11-net-stack/testing.md, The packet session drops a copy of a packet).
#[derive(Debug, Default)]
pub(crate) struct ReceivedWindow {
    /// The largest number opened, accepted or not, which the next truncated number reads against.
    largest_opened: Option<PacketNumber>,
    /// The largest number accepted, and when it arrived.
    largest_accepted: Option<(PacketNumber, Instant)>,
    /// Bit `i` set: number `largest − 1 − i` was accepted.
    bits: u64,
    /// Bit `i` set: number `largest_opened − i` is a payload opened and not settled, which waits
    /// for its frame to accept or refuse it.
    pending: u64,
    /// When the oldest accepted packet that needs an ACK, and that no sent ACK named yet, arrived.
    ack_pending_since: Option<Instant>,
    /// The accepted packets that need an ACK and that no sent ACK named yet.
    unacknowledged: u64,
}

impl ReceivedWindow {
    /// The whole number of the peer's packet whose low 16 bits are `truncated`.
    pub(crate) const fn expand(&self, truncated: u16) -> PacketNumber {
        packet_numbers::expand(self.largest_opened, truncated)
    }

    /// Whether `number` is neither accepted nor too old to tell: a packet the window does not
    /// refuse as a replay.
    pub(crate) const fn is_new(&self, number: PacketNumber) -> bool {
        let Some((largest, _)) = self.largest_accepted else {
            return true;
        };
        if number.0 > largest.0 {
            return true;
        }
        let below = largest.0 - number.0;
        below != 0 && below <= WINDOW && self.bits & (1 << (below - 1)) == 0
    }

    /// Whether `number` is above every number opened so far.
    pub(crate) fn is_largest(&self, number: PacketNumber) -> bool {
        self.largest_opened.is_none_or(|largest| number > largest)
    }

    /// Notes that `number` was opened, which the next truncated number reads against; a new
    /// largest moves the pending bits with it.
    pub(crate) fn opened(&mut self, number: PacketNumber) {
        match self.largest_opened {
            Some(largest) if number <= largest => {}
            Some(largest) => {
                let shift = number.0 - largest.0;
                self.pending = if shift >= WINDOW {
                    0
                } else {
                    self.pending << shift
                };
                self.largest_opened = Some(number);
            }
            None => self.largest_opened = Some(number),
        }
    }

    /// Whether `number` is a payload opened and not settled: a copy of it is a replay.
    pub(crate) fn is_pending(&self, number: PacketNumber) -> bool {
        self.pending_bit(number)
            .is_some_and(|bit| self.pending & bit != 0)
    }

    /// Whether a payload numbered `number` lies below the pending bits, where neither its copy
    /// nor its place in the frame's wait can be told.
    pub(crate) fn is_below_pending(&self, number: PacketNumber) -> bool {
        self.largest_opened
            .is_some_and(|largest| largest.0.saturating_sub(number.0) >= WINDOW)
    }

    /// The lowest number the pending bits hold, once a number was opened.
    pub(crate) fn pending_floor(&self) -> Option<PacketNumber> {
        self.largest_opened
            .map(|largest| PacketNumber(largest.0.saturating_sub(WINDOW - 1)))
    }

    /// Notes that the payload `number`, opened, waits for its frame.
    pub(crate) fn hold(&mut self, number: PacketNumber) {
        let bit = self.pending_bit(number);
        debug_assert!(bit.is_some(), "a payload is held within the pending bits");
        self.pending |= bit.unwrap_or(0);
    }

    /// Notes that the frame accepted or refused the payload `number`, if it is held still.
    pub(crate) fn settle(&mut self, number: PacketNumber) {
        if let Some(bit) = self.pending_bit(number) {
            self.pending &= !bit;
        }
    }

    fn pending_bit(&self, number: PacketNumber) -> Option<u64> {
        let below = self.largest_opened?.0.checked_sub(number.0)?;
        (below < WINDOW).then(|| 1 << below)
    }

    /// Accepts `number`, which arrived at `received_at`: the next ACK names it, and a packet that
    /// needs an ACK starts the ACK delay if none runs. `number` is new.
    pub(crate) fn accept(
        &mut self,
        number: PacketNumber,
        received_at: Instant,
        ack_eliciting: bool,
    ) {
        debug_assert!(self.is_new(number), "a window accepts a number once");
        match self.largest_accepted {
            Some((largest, _)) if number < largest => {
                self.bits |= 1 << (largest.0 - number.0 - 1);
            }
            Some((largest, _)) => {
                let shift = number.0 - largest.0;
                let moved = if shift >= WINDOW {
                    0
                } else {
                    self.bits << shift
                };
                self.bits = if shift > WINDOW {
                    0
                } else {
                    moved | (1 << (shift - 1))
                };
                self.largest_accepted = Some((number, received_at));
            }
            None => self.largest_accepted = Some((number, received_at)),
        }
        if ack_eliciting {
            self.unacknowledged += 1;
        }
        if ack_eliciting && self.ack_pending_since.is_none() {
            self.ack_pending_since = Some(received_at);
        }
    }

    /// The acknowledgements a packet sent at `now` carries, once a packet was accepted: the ACK
    /// delay is the time the largest waited.
    pub(crate) fn ack_fields(&self, now: Instant) -> Option<AckFields> {
        let (largest, received_at) = self.largest_accepted?;
        Some(AckFields::new(
            packet_numbers::truncate(largest),
            self.bits,
            now.saturating_duration_since(received_at),
        ))
    }

    /// A packet with this window's acknowledgements was sent.
    pub(crate) const fn acks_sent(&mut self) {
        self.ack_pending_since = None;
        self.unacknowledged = 0;
    }

    /// Whether the packets no sent ACK named pass half the window, so an ACK goes at once, before
    /// another frame's would leave the oldest unnamed, as QUIC acknowledges every second packet
    /// (RFC 9000 §13.2.2).
    pub(crate) const fn acks_due_now(&self) -> bool {
        self.unacknowledged > WINDOW / 2
    }

    /// When an ACK-only packet is due, as an accepted packet that needs an ACK waits for one.
    pub(crate) fn ack_deadline(&self) -> Option<Instant> {
        self.ack_pending_since.map(|since| since + MAX_ACK_DELAY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(millis: u64) -> Instant {
        Instant::from_elapsed(Duration::from_millis(millis))
    }

    /// Accepting 5, then 7, then 6, then 104 leaves the largest 104 and the bits of 7, 6 and 5,
    /// which are 97 to 99 below 104, past the window: none. Before 104, the bits were 5 → bit 1
    /// (7 − 5 − 1) and 6 → bit 0.
    #[test]
    fn the_bits_name_each_accepted_number_below_the_largest() {
        let mut window = ReceivedWindow::default();
        for number in [5, 7, 6] {
            window.accept(PacketNumber(number), at(number), true);
        }
        let acks = window.ack_fields(at(10)).unwrap();
        assert_eq!(acks.largest, 7);
        assert_eq!(acks.bits, 0b11);
        // 7 arrived at 7 ms, so it waited 3 ms by 10 ms.
        assert_eq!(acks.delay(), Some(Duration::from_millis(3)));
        window.accept(PacketNumber(104), at(104), true);
        let acks = window.ack_fields(at(104)).unwrap();
        assert_eq!([u64::from(acks.largest), acks.bits], [104, 0]);
        // 103 is new; 40 is 64 below 104, the window's last bit, and new; 39 is 65 below, too old.
        for (number, new) in [(103, true), (40, true), (39, false), (104, false)] {
            assert_eq!(window.is_new(PacketNumber(number)), new, "{number}");
        }
    }

    /// A largest 64 above the last shifts the old largest into the top bit, and one 65 above
    /// shifts it out.
    #[test]
    fn a_jump_of_64_keeps_the_old_largest_and_65_drops_it() {
        let mut window = ReceivedWindow::default();
        window.accept(PacketNumber(0), at(0), false);
        window.accept(PacketNumber(64), at(1), false);
        assert_eq!(window.ack_fields(at(1)).unwrap().bits, 1 << 63);
        window.accept(PacketNumber(129), at(2), false);
        assert_eq!(window.ack_fields(at(2)).unwrap().bits, 0);
    }

    /// Only a packet that needs an ACK starts the delay, at its arrival, and a later one does not
    /// move it; a sent ACK ends it.
    #[test]
    fn the_ack_delay_runs_from_the_oldest_unacknowledged_packet() {
        let mut window = ReceivedWindow::default();
        window.accept(PacketNumber(1), at(1), false);
        assert_eq!(window.ack_deadline(), None);
        window.accept(PacketNumber(2), at(2), true);
        window.accept(PacketNumber(3), at(9), true);
        assert_eq!(window.ack_deadline(), Some(at(2) + MAX_ACK_DELAY));
        window.acks_sent();
        assert_eq!(window.ack_deadline(), None);
    }

    /// Payloads 10 and 12, held, are pending, and 11 is not; a settle of 10 ends its wait. A new
    /// largest 63 above 12 keeps 12 at the last bit, and 64 above shifts it out, below the bits.
    #[test]
    fn the_pending_bits_hold_each_waiting_payload() {
        let mut window = ReceivedWindow::default();
        for number in [10, 12] {
            window.opened(PacketNumber(number));
            window.hold(PacketNumber(number));
        }
        let pending = |window: &ReceivedWindow, number| window.is_pending(PacketNumber(number));
        assert_eq!(
            [10, 11, 12].map(|n| pending(&window, n)),
            [true, false, true]
        );
        window.settle(PacketNumber(10));
        assert!(!pending(&window, 10));

        window.opened(PacketNumber(12 + 63));
        assert!(pending(&window, 12));
        assert!(!window.is_below_pending(PacketNumber(12)));
        assert_eq!(window.pending_floor(), Some(PacketNumber(12)));
        window.opened(PacketNumber(12 + 64));
        assert!(!pending(&window, 12));
        assert!(window.is_below_pending(PacketNumber(12)));
    }

    /// A number reads against the largest opened, accepted or not.
    #[test]
    fn a_truncated_number_reads_against_the_largest_opened() {
        let mut window = ReceivedWindow::default();
        window.opened(PacketNumber(0x1_fff0));
        assert_eq!(window.expand(0x0005), PacketNumber(0x2_0005));
    }
}
