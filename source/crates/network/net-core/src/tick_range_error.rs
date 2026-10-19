use core::fmt::{Display, Formatter};

/// A tick value lies outside the range of the tick type it is converted to: a negative delta as
/// an instant, or a count of 2^31 ticks or more as a signed delta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickRangeError;

impl Display for TickRangeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.write_str("the tick value lies outside the target type's range")
    }
}

impl core::error::Error for TickRangeError {}
