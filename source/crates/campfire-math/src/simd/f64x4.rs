#![expect(
    clippy::float_arithmetic,
    reason = "each lane's float is exact by its domain, or an estimate that integer steps correct"
)]

use crate::simd::i64x4::I64x4;
use crate::simd::u64x4::U64x4;

/// Four `f64` lanes, for `math`'s own exact ops only. AVX2 converts no `i64` to `f64` in lanes,
/// so each conversion adds an integer to the bits of a power of two whose last place is 1.
#[derive(Debug, Clone, Copy)]
pub(crate) struct F64x4([f64; 4]);

/// 2⁵², whose last place is 1: an integer from 0 to 2⁵² − 1 fills its low bits.
const TWO_52: f64 = 4_503_599_627_370_496.0;
/// 2⁸⁴, whose last place is 2³²: an integer below 2³² in its low bits counts 2³² each.
const TWO_84: f64 = 19_342_813_113_834_066_795_298_816.0;
/// 1.5 · 2⁵², whose last place is 1: an integer within 2⁵¹ either side moves only its low bits.
const SIGNED_52: f64 = 6_755_399_441_055_744.0;

impl F64x4 {
    /// Each lane exactly; each below 2⁵¹ in magnitude.
    pub(crate) const fn from_exact(lanes: I64x4) -> F64x4 {
        let [a, b, c, d] = lanes.to_array();
        F64x4([
            F64x4::exact(a),
            F64x4::exact(b),
            F64x4::exact(c),
            F64x4::exact(d),
        ])
    }

    /// Each lane rounded once to the nearest `f64`, for every `u64`: its two halves convert
    /// exactly, and only their sum rounds.
    pub(crate) const fn from_u64(lanes: U64x4) -> F64x4 {
        let [a, b, c, d] = lanes.to_array();
        F64x4([
            F64x4::rounded(a),
            F64x4::rounded(b),
            F64x4::rounded(c),
            F64x4::rounded(d),
        ])
    }

    pub(crate) const fn div(self, rhs: F64x4) -> F64x4 {
        let [a, b] = [self.0, rhs.0];
        F64x4([a[0] / b[0], a[1] / b[1], a[2] / b[2], a[3] / b[3]])
    }

    pub(crate) const fn floor(self) -> F64x4 {
        let [a, b, c, d] = self.0;
        F64x4([a.floor(), b.floor(), c.floor(), d.floor()])
    }

    pub(crate) fn sqrt(self) -> F64x4 {
        F64x4(self.0.map(f64::sqrt))
    }

    /// Each lane, a whole number below 2⁵¹ in magnitude, as an `i64`.
    pub(crate) const fn to_exact(self) -> I64x4 {
        let [a, b, c, d] = self.0;
        I64x4::from_array([
            F64x4::whole(a),
            F64x4::whole(b),
            F64x4::whole(c),
            F64x4::whole(d),
        ])
    }

    /// Each lane, from 0 to below 2⁵², rounded to the nearest integer, ties to even.
    pub(crate) const fn round_to_u64(self) -> U64x4 {
        let [a, b, c, d] = self.0;
        U64x4::from_array([
            F64x4::nearest(a),
            F64x4::nearest(b),
            F64x4::nearest(c),
            F64x4::nearest(d),
        ])
    }

    const fn exact(lane: i64) -> f64 {
        debug_assert!(lane.unsigned_abs() < 1 << 51);
        f64::from_bits(SIGNED_52.to_bits().wrapping_add_signed(lane)) - SIGNED_52
    }

    const fn rounded(lane: u64) -> f64 {
        let high = f64::from_bits(TWO_84.to_bits() | lane >> 32) - TWO_84;
        let low = f64::from_bits(TWO_52.to_bits() | lane & 0xFFFF_FFFF) - TWO_52;
        high + low
    }

    const fn whole(lane: f64) -> i64 {
        debug_assert!(lane.abs() < TWO_52 / 2.0 && lane % 1.0 == 0.0);
        (lane + SIGNED_52)
            .to_bits()
            .wrapping_sub(SIGNED_52.to_bits())
            .cast_signed()
    }

    const fn nearest(lane: f64) -> u64 {
        debug_assert!(lane >= 0.0 && lane < TWO_52);
        (lane + TWO_52).to_bits().wrapping_sub(TWO_52.to_bits())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_constants_are_their_powers_of_two() {
        assert_eq!(TWO_52, 2_f64.powi(52));
        assert_eq!(TWO_84, 2_f64.powi(84));
        assert_eq!(SIGNED_52, 1.5 * 2_f64.powi(52));
    }

    #[test]
    #[expect(clippy::cast_precision_loss, reason = "the reference conversion")]
    fn exact_conversions_hold_every_lane_to_the_edges_of_their_domain() {
        // ±(2⁵¹ − 1), the widest lanes, both ways, and 0 and ±1.
        let edge = (1_i64 << 51) - 1;
        let lanes = [[-edge, -1, 0, edge], [1, -2, 1 << 50, -(1 << 50)]];
        for lanes in lanes {
            let float = F64x4::from_exact(I64x4::from_array(lanes));
            assert_eq!(float.0, lanes.map(|lane| lane as f64));
            assert_eq!(float.to_exact().to_array(), lanes);
        }
    }

    #[test]
    #[expect(clippy::cast_precision_loss, reason = "the reference conversion")]
    fn a_u64_rounds_once_to_the_nearest_f64() {
        // Below 2⁵³ every integer is exact; 2⁵³ + 1 is a tie between 2⁵³ and 2⁵³ + 2, to even
        // 2⁵³; 2⁶⁴ − 1 rounds up to 2⁶⁴, as `as f64` does.
        let lanes = [
            (1 << 53) - 1,
            (1 << 53) + 1,
            u64::MAX,
            0x8000_0000_0000_0401,
        ];
        let float = F64x4::from_u64(U64x4::from_array(lanes));
        assert_eq!(float.0, lanes.map(|lane| lane as f64));
        assert_eq!(float.0[1], 2_f64.powi(53));
        assert_eq!(float.0[2], 2_f64.powi(64));
    }

    #[test]
    fn a_lane_rounds_to_the_nearest_integer_ties_to_even() {
        // 2⁵² − ½ is a tie between 2⁵² − 1 and 2⁵², to even 2⁵².
        let float = F64x4([0.0, 2.5, 3.5, TWO_52 - 0.5]);
        assert_eq!(float.round_to_u64().to_array(), [0, 2, 4, 1 << 52]);
        let float = F64x4([7.49, 7.51, 0.5, 4_294_967_295.6]);
        assert_eq!(float.round_to_u64().to_array(), [7, 8, 0, 1 << 32]);
    }

    #[test]
    fn floor_sqrt_and_div_are_each_lanes() {
        let float = F64x4([-1.5, 1.5, 9.0, 2.0]);
        assert_eq!(float.floor().0, [-2.0, 1.0, 9.0, 2.0]);
        assert_eq!(
            F64x4([0.0, 4.0, 9.0, 2.0]).sqrt().0,
            [0.0, 2.0, 3.0, 2_f64.sqrt()]
        );
        assert_eq!(float.div(F64x4([2.0; 4])).0, [-0.75, 0.75, 4.5, 1.0]);
    }
}
