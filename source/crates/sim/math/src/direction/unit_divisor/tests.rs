use proptest::prelude::*;

use super::*;
use crate::direction::FRACTION;
use crate::rounding::Rounding;

/// The cases of each property a run, as the other tests of the crate draw.
const CASES: u32 = 10_000;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    /// The float estimate, which ARM takes, is the exact division, rounded to nearest, ties to
    /// even, for every root from 2⁶² to 2⁶³ and every shift and axis a direction takes, with the
    /// estimate's reciprocal off by up to its bound.
    #[test]
    fn the_estimate_is_the_exact_division(
        root in (1_u64 << 62)..=(1 << 63),
        fine in 0_u32..=62,
        width in 1_u32..=47,
        bits in any::<i64>(),
        error in -1_i64..=1,
    ) {
        // An axis below 2^(width − 1) and a fine shift of at most 61 − width keep the axis at
        // most the length, whose root at this scale is `root`, at least 2⁶².
        let fine = fine % (62 - width);
        let axis = i128::from(bits >> (64 - width));
        let shift = FRACTION + fine;
        #[expect(clippy::cast_precision_loss, clippy::float_arithmetic, reason = "the reciprocal")]
        let half = f64::from_bits((0.5 / root as f64).to_bits().wrapping_add_signed(error));
        let exact = Rounding::NearestEven.divide(axis << shift, i128::from(root));
        prop_assert_eq!(i128::from(portable::UnitDivisor::new(root, half, shift).unit(axis)), exact);
    }
}
