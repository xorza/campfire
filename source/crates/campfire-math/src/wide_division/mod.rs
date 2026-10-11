//! The division of two `u128`s, exact on every platform, each by its fastest way: x86-64 divides
//! 128 by 64 bits in one instruction, which Rust's `u128` division takes for a divisor and a
//! quotient that fit 64 bits; ARM has no such instruction, and for those its library division
//! takes longer than a float estimate and its exact correction.

#[cfg(any(test, not(target_arch = "x86_64")))]
mod aarch64;
#[cfg(target_arch = "x86_64")]
mod x86_64;

#[cfg(not(target_arch = "x86_64"))]
use crate::wide_division::aarch64 as platform;
#[cfg(target_arch = "x86_64")]
use crate::wide_division::x86_64 as platform;

/// A quotient and its rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WideDivision {
    pub(crate) quotient: u128,
    pub(crate) rest: u128,
}

impl WideDivision {
    /// `numerator / divisor` and its rest, for a divisor above 0.
    #[inline]
    pub(crate) fn of(numerator: u128, divisor: u128) -> WideDivision {
        debug_assert!(divisor > 0);
        platform::divide(numerator, divisor)
    }
}

#[cfg(test)]
mod tests;
