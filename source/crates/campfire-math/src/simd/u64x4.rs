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
mod tests {
    use proptest::prelude::*;

    use super::*;

    /// The cases of each property a run, as the other tests of the crate draw.
    const CASES: u32 = 10_000;

    /// Each lane's root against `u128`'s, four values at a time; the last group padded with 0.
    fn assert_roots(values: &[u64]) {
        for group in values.chunks(4) {
            let mut lanes = [0; 4];
            lanes[..group.len()].copy_from_slice(group);
            let roots = U64x4(lanes).floor_root().0;
            for (value, root) in lanes.into_iter().zip(roots) {
                assert_eq!(u128::from(root), u128::from(value).floor_root(), "{value}");
            }
        }
    }

    #[test]
    fn the_root_is_exact_at_the_edges_of_u64() {
        // 2⁵³ ± 1, where the float stops holding every value, 2⁶² and 2⁶³, and 2⁶⁴ − 1, whose
        // estimate rounds to 2³²; its root is 2³² − 1.
        assert_roots(&[0, 1, 2, 3, 4]);
        assert_roots(&[(1 << 53) - 1, 1 << 53, (1 << 53) + 1, 1 << 62]);
        assert_roots(&[1 << 63, (1 << 63) - 1, u64::MAX - 1, u64::MAX]);
        assert_eq!(U64x4([u64::MAX; 4]).floor_root().0, [(1 << 32) - 1; 4]);
    }

    #[test]
    fn the_root_is_exact_around_each_square() {
        // k² − 1, k² and k² + 2k = (k + 1)² − 1 for k near 2²⁶, where the float's rounding
        // starts to count, near 2³¹, and up to 2³² − 1, whose k² + 2k is u64::MAX.
        let near = |k: u64, span: u64| k - span..k + span.min((1 << 32) - k);
        let roots = (1_u64..2000)
            .chain(near(1 << 26, 2000))
            .chain(near(1 << 31, 2000))
            .chain((1 << 32) - 4000..1 << 32);
        let values: Vec<u64> = roots
            .flat_map(|k| [k * k - 1, k * k, k * k + 2 * k])
            .collect();
        assert_roots(&values);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(CASES))]

        #[test]
        fn the_root_is_floor_roots_at_every_width(
            bits in prop::array::uniform4(any::<u64>()),
            shift in 0_u32..64,
        ) {
            let lanes = bits.map(|bits| bits >> shift);
            let roots = U64x4(lanes).floor_root().0;
            let expected = lanes.map(|value| u128::from(value).floor_root());
            prop_assert_eq!(roots.map(u128::from), expected);
        }
    }

    #[test]
    fn casts_keep_each_lanes_bits() {
        let lanes = [0, 1, 1 << 63, u64::MAX];
        assert_eq!(U64x4(lanes).cast_signed().to_array(), [0, 1, i64::MIN, -1]);
        assert_eq!(U64x4(lanes).cast_signed().cast_unsigned(), U64x4(lanes));
    }
}
