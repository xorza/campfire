//! The bound of a place's coordinates, which the vectors and the directions derive from.

use crate::num::Num;

/// The bits of the bound of a place's coordinates: 2²⁰ m at `Num`'s 24 bits of fraction.
pub(crate) const BOUND_BITS: u32 = 20 + Num::FRAC_BITS;

/// The widest axis of a vector a direction is taken from, or of an offset it measures,
/// exclusive, in bits: places lie within the bound, so offsets within twice it, and a box's at
/// twice their scale within four times it, below eight times it, 2⁴⁷.
pub(crate) const WIDEST: i128 = 1 << (BOUND_BITS + 3);
