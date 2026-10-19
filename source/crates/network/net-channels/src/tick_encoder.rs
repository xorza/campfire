use campfire_net_core::NetTick;

/// The bytes of a tick in its short form: 15 bits of it, read against the receiver's largest.
pub(crate) const SHORT_TICK_LEN: usize = 2;

/// The bytes of a tick in its long form: a marker byte and the whole tick.
pub(crate) const LONG_TICK_LEN: usize = 5;

/// The top bit of a tick's first byte, set in the long form.
pub(crate) const LONG_TICK_MARK: u8 = 0x80;

/// The ticks the short form names apart: 2^15, of which a receiver reads the nearest to its own
/// largest.
pub(crate) const SHORT_TICK_WINDOW: u32 = 1 << 15;

/// A tick as a packet carries it, after the session's headroom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EncodedTick {
    bytes: [u8; LONG_TICK_LEN],
    len: usize,
}

impl EncodedTick {
    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

/// Writes each packet's tick in 2 bytes once the peer surely reads it whole, as RFC 9000 §A.2
/// writes a packet number: the receiver's reference, the largest tick it read, lies between the
/// largest tick the peer accepted and the largest sent; a tick within half the short window of
/// both is the nearest one to that reference, whatever it is. Any other tick goes whole.
#[derive(Debug, Default)]
pub(crate) struct TickEncoder {
    /// The largest tick of a packet the peer accepted.
    delivered: Option<NetTick>,
    /// The largest tick sent.
    largest_sent: Option<NetTick>,
}

impl TickEncoder {
    /// The form of `tick` for the packets of one flush.
    pub(crate) fn encode(&self, tick: NetTick) -> EncodedTick {
        let near = |reference: Option<NetTick>| {
            reference.is_some_and(|reference| tick.0.abs_diff(reference.0) < SHORT_TICK_WINDOW / 2)
        };
        let mut bytes = [0; LONG_TICK_LEN];
        if near(self.delivered) && near(self.largest_sent) {
            let short = u16::try_from(tick.0 % SHORT_TICK_WINDOW).expect("15 bits fit a u16");
            bytes[..SHORT_TICK_LEN].copy_from_slice(&short.to_be_bytes());
            EncodedTick {
                bytes,
                len: SHORT_TICK_LEN,
            }
        } else {
            bytes[0] = LONG_TICK_MARK;
            bytes[1..].copy_from_slice(&tick.0.to_be_bytes());
            EncodedTick {
                bytes,
                len: LONG_TICK_LEN,
            }
        }
    }

    /// Records a flush that sent packets at `tick`.
    pub(crate) fn on_sent(&mut self, tick: NetTick) {
        self.largest_sent = Some(self.largest_sent.map_or(tick, |largest| largest.max(tick)));
    }

    /// Records that the peer accepted a packet of `tick`.
    pub(crate) fn on_delivered(&mut self, tick: NetTick) {
        self.delivered = Some(self.delivered.map_or(tick, |delivered| delivered.max(tick)));
    }

    pub(crate) const fn reset(&mut self) {
        self.delivered = None;
        self.largest_sent = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Before the peer accepted a packet, each tick goes whole; once it accepted the one of tick
    /// 100, tick 101 goes in 2 bytes, its low 15 bits.
    #[test]
    fn a_tick_goes_short_once_the_peer_accepted_one() {
        let mut encoder = TickEncoder::default();
        assert_eq!(
            encoder.encode(NetTick(100)).as_bytes(),
            [0x80, 0, 0, 0, 100]
        );
        encoder.on_sent(NetTick(100));
        assert_eq!(encoder.encode(NetTick(101)).len, LONG_TICK_LEN);
        encoder.on_delivered(NetTick(100));
        assert_eq!(encoder.encode(NetTick(101)).as_bytes(), [0, 101]);
        // 0x1_8005 keeps its low 15 bits, 0x0005, but is too far from 100 to go short.
        assert_eq!(encoder.encode(NetTick(0x1_8005)).len, LONG_TICK_LEN);
    }

    /// A tick within 2^14 of the accepted one goes short, and one 2^14 away goes whole; a sent
    /// tick far past the accepted one sends the next whole too, as the receiver may read it.
    #[test]
    fn a_tick_goes_short_only_near_the_accepted_and_the_sent() {
        let mut encoder = TickEncoder::default();
        encoder.on_sent(NetTick(20_000));
        encoder.on_delivered(NetTick(20_000));
        assert_eq!(encoder.encode(NetTick(20_000 + 16_383)).len, SHORT_TICK_LEN);
        assert_eq!(encoder.encode(NetTick(20_000 + 16_384)).len, LONG_TICK_LEN);
        assert_eq!(encoder.encode(NetTick(20_000 - 16_383)).len, SHORT_TICK_LEN);
        encoder.on_sent(NetTick(40_000));
        assert_eq!(encoder.encode(NetTick(23_000)).len, LONG_TICK_LEN);
        assert_eq!(encoder.encode(NetTick(30_000)).len, SHORT_TICK_LEN);
    }

    /// The delivered tick only grows: a late delivery of an older packet, of tick 10 after 11,
    /// leaves 11.
    #[test]
    fn a_late_delivery_keeps_the_largest_tick() {
        let mut encoder = TickEncoder::default();
        encoder.on_delivered(NetTick(11));
        encoder.on_delivered(NetTick(10));
        assert_eq!(encoder.delivered, Some(NetTick(11)));
    }
}
