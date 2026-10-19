use campfire_net_bytes::{ReadInteger, Reader, SerializationError};
use campfire_net_core::NetTick;

use crate::packet_error::PacketError;
use crate::tick_encoder::{LONG_TICK_MARK, SHORT_TICK_WINDOW};

/// Reads each packet's tick, a short one as the nearest tick to the largest it read, as RFC 9000
/// §A.3 reads a packet number.
#[derive(Debug, Default)]
pub(crate) struct TickDecoder {
    largest: Option<NetTick>,
}

impl TickDecoder {
    /// Reads the tick that starts `reader`; a short tick before any whole one breaks the
    /// protocol, as the sender sends none before the peer read one.
    pub(crate) fn read(&mut self, reader: &mut Reader) -> Result<NetTick, PacketError> {
        let first = reader.read_u8().map_err(SerializationError::from)?;
        let tick = if first & LONG_TICK_MARK != 0 {
            if first != LONG_TICK_MARK {
                return Err(PacketError::MalformedTick);
            }
            NetTick(reader.read_u32().map_err(SerializationError::from)?)
        } else {
            let second = reader.read_u8().map_err(SerializationError::from)?;
            let short = u32::from(u16::from_be_bytes([first, second]));
            let largest = self.largest.ok_or(PacketError::MalformedTick)?;
            NetTick(nearest(largest.0, short))
        };
        self.largest = Some(self.largest.map_or(tick, |largest| largest.max(tick)));
        Ok(tick)
    }

    pub(crate) const fn reset(&mut self) {
        self.largest = None;
    }
}

/// The tick nearest to `expected` whose low 15 bits are `short`, within the `u32` range.
const fn nearest(expected: u32, short: u32) -> u32 {
    const HALF: u32 = SHORT_TICK_WINDOW / 2;
    let candidate = (expected & !(SHORT_TICK_WINDOW - 1)) | short;
    if candidate.saturating_add(HALF) <= expected && candidate <= u32::MAX - SHORT_TICK_WINDOW {
        candidate + SHORT_TICK_WINDOW
    } else if candidate > expected.saturating_add(HALF) && candidate >= SHORT_TICK_WINDOW {
        candidate - SHORT_TICK_WINDOW
    } else {
        candidate
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tick_encoder::TickEncoder;

    fn read(decoder: &mut TickDecoder, bytes: &[u8]) -> Result<NetTick, PacketError> {
        decoder.read(&mut Reader::from(bytes.to_vec()))
    }

    /// A short tick is the nearest to the largest read, across a wrap of its 15 bits each way:
    /// from 0x7FF0, 0x0005 is 0x8005; from 0x8005, 0x7FF0 is 0x7FF0 again.
    #[test]
    fn a_short_tick_is_the_nearest_to_the_largest_read() {
        assert_eq!(nearest(0x7FF0, 0x0005), 0x8005);
        assert_eq!(nearest(0x8005, 0x7FF0), 0x7FF0);
        assert_eq!(nearest(100, 101), 101);
        assert_eq!(nearest(0, 0x7FFF), 0x7FFF);
        assert_eq!(nearest(u32::MAX, 0x0000), u32::MAX - 0x7FFF);
    }

    /// What the encoder writes, the decoder reads back: a whole tick, then short ones near it,
    /// over the wrap of the low 15 bits.
    #[test]
    fn the_decoder_reads_what_the_encoder_writes() {
        let mut encoder = TickEncoder::default();
        let mut decoder = TickDecoder::default();
        for tick in [0x7FF0_u32, 0x7FF8, 0x8003, 0x8010] {
            let encoded = encoder.encode(NetTick(tick));
            assert_eq!(
                read(&mut decoder, encoded.as_bytes()).unwrap(),
                NetTick(tick)
            );
            encoder.on_sent(NetTick(tick));
            encoder.on_delivered(NetTick(tick));
        }
    }

    /// A short tick before a whole one, and a long form with another bit set, break the protocol.
    #[test]
    fn a_tick_with_no_reference_or_a_bad_mark_is_refused() {
        let mut decoder = TickDecoder::default();
        assert!(matches!(
            read(&mut decoder, &[0, 1]),
            Err(PacketError::MalformedTick)
        ));
        assert!(matches!(
            read(&mut decoder, &[0x81, 0, 0, 0, 1]),
            Err(PacketError::MalformedTick)
        ));
        assert_eq!(read(&mut decoder, &[0x80, 0, 0, 0, 1]).unwrap(), NetTick(1));
    }
}
