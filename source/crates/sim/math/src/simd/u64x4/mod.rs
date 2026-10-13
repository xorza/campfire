use crate::floor_root::FloorRoot;
use crate::simd::f64x4::F64x4;
use crate::simd::i64x4::I64x4;

/// Four `u64` lanes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct U64x4([u64; 4]);

impl U64x4 {
    pub const fn from_array(lanes: [u64; 4]) -> U64x4 {
        U64x4(lanes)
    }

    pub const fn to_array(self) -> [u64; 4] {
        self.0
    }

    pub const fn cast_signed(self) -> I64x4 {
        let [a, b, c, d] = self.0;
        I64x4::from_array([
            a.cast_signed(),
            b.cast_signed(),
            c.cast_signed(),
            d.cast_signed(),
        ])
    }

    /// The floor root of `value` from `estimate`, the floor or one above it, at most 2³².
    const fn corrected(value: u64, estimate: u64) -> u64 {
        debug_assert!(estimate <= 1 << 32);
        // 2³² becomes 2³² − 1, the largest root, so the square below fits.
        let root = estimate.wrapping_sub(estimate >> 32);
        // The mask shows LLVM what the clamp proves, so the square is one widening multiply.
        let narrow = root & 0xFFFF_FFFF;
        root.wrapping_sub((narrow.wrapping_mul(narrow) > value) as u64)
    }
}

impl FloorRoot for U64x4 {
    /// Exact for every `u64`. The value rounds once to `f64`, by at most 2⁻⁵³ of itself, and the
    /// root once more, so the estimate lies within 2⁻²⁰ of the root, below 2³²; rounded to the
    /// nearest integer, it is the floor or one above it, and one step down corrects it.
    fn floor_root(self) -> U64x4 {
        let estimate = F64x4::from_u64(self).sqrt().round_to_u64().to_array();
        let [a, b, c, d] = self.0;
        U64x4([
            U64x4::corrected(a, estimate[0]),
            U64x4::corrected(b, estimate[1]),
            U64x4::corrected(c, estimate[2]),
            U64x4::corrected(d, estimate[3]),
        ])
    }
}

#[cfg(test)]
mod tests;
