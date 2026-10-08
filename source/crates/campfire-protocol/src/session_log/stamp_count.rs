use campfire_common::Tick;
use serde::{Deserialize, Serialize};

use crate::session_log::error::InputError;

/// A player's inputs as their stamps count them: the last stamp, the inputs of that stamp, and
/// the inputs in all.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StampCount {
    pub(super) last: Option<Tick>,
    pub(super) at_last: u32,
    pub(super) total: u64,
}

impl StampCount {
    /// Counts one more input, stamped `stamp`; an error for a stamp before the last, or one
    /// more than `max` inputs of one stamp.
    pub(super) const fn add(&mut self, stamp: Tick, max: u32) -> Result<(), InputError> {
        match self.last {
            Some(last) if stamp.get() < last.get() => return Err(InputError::StampBack),
            Some(last) if stamp.get() == last.get() => {
                if self.at_last == max {
                    return Err(InputError::TooManyInputs);
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
