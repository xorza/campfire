use core::fmt::{Display, Formatter};

/// A tick value lies outside the range of the tick type it is converted to: a negative span as an
/// instant, a count of 2^31 ticks or more as a signed span, or a value outside `[0, 1)` as an
/// overstep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickRangeError;

impl Display for TickRangeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.write_str("the tick value lies outside the target type's range")
    }
}

impl core::error::Error for TickRangeError {}
