use campfire_common::Tick;
use serde::{Deserialize, Serialize};

use crate::stamp_count::error::StampError;

pub(crate) mod error;

/// A player's inputs as their stamps count them: the last stamp, the inputs of that stamp, and
/// the inputs in all.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StampCount {
    pub(crate) last: Option<Tick>,
    pub(crate) at_last: u32,
    pub(crate) total: u64,
}

impl StampCount {
    /// Counts one more input, stamped `stamp`; an error for a stamp before the last, or one
    /// more than `max` inputs of one stamp.
    pub(crate) const fn add(&mut self, stamp: Tick, max: u32) -> Result<(), StampError> {
        match self.last {
            Some(last) if stamp.get() < last.get() => return Err(StampError::Back),
            Some(last) if stamp.get() == last.get() => {
                if self.at_last == max {
                    return Err(StampError::TooMany);
                }
                self.at_last += 1;
            }
            _ => {
                self.last = Some(stamp);
                self.at_last = 1;
            }
        }
        self.total += 1;
        Ok(())
    }
}
