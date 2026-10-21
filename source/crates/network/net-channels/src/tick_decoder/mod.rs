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
mod tests;
