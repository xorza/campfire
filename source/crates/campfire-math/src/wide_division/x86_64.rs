use crate::wide_division::WideDivision;

/// Rust's `u128` division, a library call, which for a divisor and a quotient that fit 64 bits
/// takes one `div` instruction.
#[inline]
pub(super) const fn divide(numerator: u128, divisor: u128) -> WideDivision {
    WideDivision {
        quotient: numerator / divisor,
        rest: numerator % divisor,
    }
}
