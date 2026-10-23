use campfire_net_bytes::Reader;
use campfire_net_core::{NetTick, Overstep, TickInstant};

use crate::error::ReceiveError;
use crate::tick_encoder::{LONG_TICK_MARK, SHORT_TICK_WINDOW};

/// Reads each packet's sender instant: its tick, a short one as the nearest tick to the largest it
/// read, as RFC 9000 §A.3 reads a packet number, then its overstep.
#[derive(Debug, Default)]
pub(crate) struct TickDecoder {
    largest: Option<NetTick>,
}

impl TickDecoder {
    /// Reads the instant that starts `reader`; a short tick before any whole one breaks the
    /// protocol, as the sender sends none before the peer read one.
    pub(crate) fn read(&mut self, reader: &mut Reader) -> Result<TickInstant, ReceiveError> {
        let first = reader.read_u8()?;
        let tick = if first & LONG_TICK_MARK != 0 {
            if first != LONG_TICK_MARK {
                return Err(ReceiveError::MalformedTick);
            }
            NetTick(reader.read_u32()?)
        } else {
            let second = reader.read_u8()?;
            let short = u32::from(u16::from_be_bytes([first, second]));
            let largest = self.largest.ok_or(ReceiveError::MalformedTick)?;
            NetTick(nearest(largest.0, short))
        };
        self.largest = Some(self.largest.map_or(tick, |largest| largest.max(tick)));
        let overstep = Overstep::from_u8(reader.read_u8()?);
        Ok(TickInstant::from_tick_and_overstep(tick, overstep))
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
mod tests;
