//! The division of a unit's axes by its root, each platform by its fastest way: x86-64 divides
//! each axis by the root with one `div` instruction, 128 bits by 64; ARM has no such
//! instruction, and takes a float estimate of each from the reciprocal the root's estimate took,
//! which integer steps correct exactly.

#[cfg(any(test, not(target_arch = "x86_64")))]
pub(super) mod aarch64;
#[cfg(target_arch = "x86_64")]
pub(super) mod x86_64;

#[cfg(test)]
mod tests;
