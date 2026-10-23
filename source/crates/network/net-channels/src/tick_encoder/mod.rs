use campfire_net_core::{NetTick, TickInstant};

/// The bytes of a tick in its short form: 15 bits of it, read against the receiver's largest.
pub(crate) const SHORT_TICK_LEN: usize = 2;

/// The bytes of a tick in its long form: a marker byte and the whole tick.
pub(crate) const LONG_TICK_LEN: usize = 5;

/// The top bit of a tick's first byte, set in the long form.
pub(crate) const LONG_TICK_MARK: u8 = 0x80;

/// The ticks the short form names apart: 2^15, of which a receiver reads the nearest to its own
/// largest.
pub(crate) const SHORT_TICK_WINDOW: u32 = 1 << 15;

/// The byte of the sender's overstep after its tick: the share of a tick it ran past the tick, a
/// step of 1/256 rounded down, 0.13 ms at 30 Hz, under a link's jitter.
pub(crate) const OVERSTEP_LEN: usize = 1;

/// The sender's instant as a packet carries it, after the packet session's headroom: its tick,
/// then its overstep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EncodedTick {
    bytes: [u8; Self::MAX_LEN],
    len: usize,
}

impl EncodedTick {
    /// The bytes of the longest form: a whole tick, then the overstep.
    pub(crate) const MAX_LEN: usize = LONG_TICK_LEN + OVERSTEP_LEN;

    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

/// Writes each packet's tick in 2 bytes once the peer surely reads it whole, as RFC 9000 §A.2
/// writes a packet number: the receiver's reference, the largest tick it read, lies between the
/// largest tick the peer accepted and the largest sent; a tick within half the short window of both
/// is the nearest one to that reference, whatever it is. Any other tick goes whole
/// (11-net-stack/transport.md, A packet's tick goes in 2 bytes once the peer surely reads it
/// whole).
#[derive(Debug, Default)]
pub(crate) struct TickEncoder {
    /// The largest tick of a packet the peer accepted.
    delivered: Option<NetTick>,
    /// The largest tick sent.
    largest_sent: Option<NetTick>,
}

impl TickEncoder {
    /// The form of `at`, the sender's instant, for the packets of one flush: its tick, short or
    /// whole, then its overstep, so the peer reads the sender's instant and not only the tick it
    /// passed, which a frame's end may leave up to a tick behind.
    pub(crate) fn encode(&self, at: TickInstant) -> EncodedTick {
        let tick = at.tick();
        let near = |reference: Option<NetTick>| {
            reference.is_some_and(|reference| tick.0.abs_diff(reference.0) < SHORT_TICK_WINDOW / 2)
        };
        let mut bytes = [0; EncodedTick::MAX_LEN];
        let len = if near(self.delivered) && near(self.largest_sent) {
            let short = u16::try_from(tick.0 % SHORT_TICK_WINDOW).expect("15 bits fit a u16");
            bytes[..SHORT_TICK_LEN].copy_from_slice(&short.to_be_bytes());
            SHORT_TICK_LEN
        } else {
            bytes[0] = LONG_TICK_MARK;
            bytes[1..LONG_TICK_LEN].copy_from_slice(&tick.0.to_be_bytes());
            LONG_TICK_LEN
        };
        bytes[len] = at.overstep().to_u8();
        EncodedTick {
            bytes,
            len: len + OVERSTEP_LEN,
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
mod tests;
