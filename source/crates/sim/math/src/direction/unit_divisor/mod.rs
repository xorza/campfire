//! The division of a unit's axes by its root, each platform by its fastest way: x86-64 divides
//! each axis by the root with one `div` instruction, 128 bits by 64; ARM has no such
//! instruction, and takes a float estimate of each from the reciprocal the root's estimate took,
//! which integer steps correct exactly.

#[cfg(any(test, not(target_arch = "x86_64")))]
mod portable;
#[cfg(target_arch = "x86_64")]
mod x86_64;

#[cfg(not(target_arch = "x86_64"))]
use crate::direction::unit_divisor::portable as platform;
#[cfg(target_arch = "x86_64")]
use crate::direction::unit_divisor::x86_64 as platform;

/// The divisor of the platform the crate builds for.
pub(super) type UnitDivisor = platform::UnitDivisor;

#[cfg(test)]
mod tests;
