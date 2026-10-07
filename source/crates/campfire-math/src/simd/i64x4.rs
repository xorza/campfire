use crate::simd::f64x4::F64x4;
use crate::simd::mask64x4::Mask64x4;
use crate::simd::u64x4::U64x4;

/// Four `i64` lanes. Each op works lane by lane and wraps; its doc states the domain inside which
/// it cannot, which a debug build asserts, so a caller checks its inputs once and takes its
/// scalar path outside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct I64x4([i64; 4]);

impl I64x4 {
    pub const fn splat(value: i64) -> I64x4 {
        I64x4([value; 4])
    }

    pub const fn from_array(lanes: [i64; 4]) -> I64x4 {
        I64x4(lanes)
    }

    pub const fn to_array(self) -> [i64; 4] {
        self.0
    }

    #[must_use]
    pub const fn wrapping_add(self, rhs: I64x4) -> I64x4 {
        let [a, b] = [self.0, rhs.0];
        I64x4([
            a[0].wrapping_add(b[0]),
            a[1].wrapping_add(b[1]),
            a[2].wrapping_add(b[2]),
            a[3].wrapping_add(b[3]),
        ])
    }

    #[must_use]
    pub const fn wrapping_sub(self, rhs: I64x4) -> I64x4 {
        let [a, b] = [self.0, rhs.0];
        I64x4([
            a[0].wrapping_sub(b[0]),
            a[1].wrapping_sub(b[1]),
            a[2].wrapping_sub(b[2]),
            a[3].wrapping_sub(b[3]),
        ])
    }

    /// The exact products of lanes that each fit an `i32`: one widening multiply, as AVX2 has
    /// none of two `i64`s.
    #[must_use]
    pub const fn mul_narrow(self, rhs: I64x4) -> I64x4 {
        let [a, b] = [self.0, rhs.0];
        I64x4([
            I64x4::narrow_product(a[0], b[0]),
            I64x4::narrow_product(a[1], b[1]),
            I64x4::narrow_product(a[2], b[2]),
            I64x4::narrow_product(a[3], b[3]),
        ])
    }

    #[must_use]
    pub const fn max(self, rhs: I64x4) -> I64x4 {
        let [a, b] = [self.0, rhs.0];
        I64x4([
            I64x4::larger(a[0], b[0]),
            I64x4::larger(a[1], b[1]),
            I64x4::larger(a[2], b[2]),
            I64x4::larger(a[3], b[3]),
        ])
    }

    #[must_use]
    pub const fn min(self, rhs: I64x4) -> I64x4 {
        let [a, b] = [self.0, rhs.0];
        I64x4([
            I64x4::smaller(a[0], b[0]),
            I64x4::smaller(a[1], b[1]),
            I64x4::smaller(a[2], b[2]),
            I64x4::smaller(a[3], b[3]),
        ])
    }

    /// Holds where `self`'s lane is at most `rhs`'s.
    pub const fn simd_le(self, rhs: I64x4) -> Mask64x4 {
        let [a, b] = [self.0, rhs.0];
        Mask64x4::from_holds([a[0] <= b[0], a[1] <= b[1], a[2] <= b[2], a[3] <= b[3]])
    }

    /// Each lane divided by `divisor`, rounded down, as `i64::div_euclid` for a positive divisor:
    /// for lanes below 2⁵¹ in magnitude and a divisor from 1 to 2⁵¹ − 1. Both convert to `f64`
    /// exactly, and the quotient's floor is exact: a quotient that is not whole lies at least
    /// 1 / `divisor` below the next whole number, more than the half of its last place by which
    /// it rounds, as the lane is below 2⁵³.
    #[must_use]
    pub const fn div_euclid(self, divisor: i64) -> I64x4 {
        debug_assert!(0 < divisor && divisor < 1 << 51);
        let divisor = F64x4::from_exact(I64x4::splat(divisor));
        F64x4::from_exact(self).div(divisor).floor().to_exact()
    }

    pub const fn cast_unsigned(self) -> U64x4 {
        let [a, b, c, d] = self.0;
        U64x4::from_array([
            a.cast_unsigned(),
            b.cast_unsigned(),
            c.cast_unsigned(),
            d.cast_unsigned(),
        ])
    }

    #[expect(clippy::cast_possible_truncation, reason = "each factor fits an i32")]
    const fn narrow_product(a: i64, b: i64) -> i64 {
        debug_assert!(a as i32 as i64 == a && b as i32 as i64 == b);
        (a as i32 as i64).wrapping_mul(b as i32 as i64)
    }

    const fn larger(a: i64, b: i64) -> i64 {
        if a < b { b } else { a }
    }

    const fn smaller(a: i64, b: i64) -> i64 {
        if a < b { a } else { b }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    /// The cases of each property a run, as the other tests of the crate draw.
    const CASES: u32 = 10_000;

    /// The widest lanes, both signs, and the lanes around 0.
    const EDGES: [i64; 8] = [i64::MIN, i64::MIN + 1, -1, 0, 1, i64::MAX - 1, i64::MAX, 7];

    /// Every lane of `op` on lanes against `scalar` on each, for each pair of `EDGES` in turn.
    fn each_pair(op: impl Fn(I64x4, I64x4) -> I64x4, scalar: impl Fn(i64, i64) -> i64) {
        for &a in EDGES.as_chunks::<4>().0 {
            for &b in EDGES.as_chunks::<4>().0 {
                for turn in 0..4 {
                    let mut b = b;
                    b.rotate_left(turn);
                    let got = op(I64x4(a), I64x4(b)).0;
                    let expected = [0, 1, 2, 3].map(|lane| scalar(a[lane], b[lane]));
                    assert_eq!(got, expected, "{a:?} {b:?}");
                }
            }
        }
    }

    #[test]
    fn the_lane_ops_are_their_scalar_ops_at_the_edges() {
        each_pair(I64x4::wrapping_add, i64::wrapping_add);
        each_pair(I64x4::wrapping_sub, i64::wrapping_sub);
        each_pair(I64x4::max, i64::max);
        each_pair(I64x4::min, i64::min);
        let le = |a: I64x4, b: I64x4| {
            let bits = a.simd_le(b).to_bits();
            I64x4([0, 1, 2, 3].map(|lane| i64::from(bits >> lane & 1)))
        };
        each_pair(le, |a, b| i64::from(a <= b));
        assert_eq!(I64x4::splat(-5).0, [-5; 4]);
        assert_eq!(
            I64x4([-1, 0, 1, i64::MIN]).cast_unsigned().to_array(),
            [u64::MAX, 0, 1, 1 << 63]
        );
    }

    #[test]
    fn narrow_products_are_exact_at_the_edges_of_i32() {
        // i32::MIN² = 2⁶², the largest; i32::MIN · i32::MAX = −2⁶² + 2³¹, the most negative.
        let a = I64x4([i32::MIN.into(), i32::MIN.into(), i32::MAX.into(), -1]);
        let b = I64x4([i32::MIN.into(), i32::MAX.into(), i32::MAX.into(), 0]);
        assert_eq!(
            a.mul_narrow(b).0,
            [
                1 << 62,
                -(1 << 62) + (1 << 31),
                (1 << 62) - (1 << 32) + 1,
                0
            ]
        );
    }

    #[test]
    fn division_rounds_down_exactly_at_the_edges_of_its_domain() {
        // ±(2⁵¹ − 1) over 1, 2, 3 and 2⁵¹ − 1; around multiples of a cell of 2²⁴, both signs;
        // −1 over anything is −1, and 0 is 0.
        let edge = (1_i64 << 51) - 1;
        let cell = 1 << 24;
        let numerators = [
            [edge, -edge, 0, -1],
            [3 * cell - 1, 3 * cell, 3 * cell + 1, -3 * cell - 1],
            [-3 * cell, -3 * cell + 1, edge - 1, -(edge - 1)],
        ];
        for divisor in [1, 2, 3, cell, 2 * cell, edge] {
            for lanes in numerators {
                let expected = lanes.map(|lane| lane.div_euclid(divisor));
                assert_eq!(
                    I64x4(lanes).div_euclid(divisor).0,
                    expected,
                    "{lanes:?} / {divisor}"
                );
            }
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(CASES))]

        #[test]
        fn division_is_div_euclid_over_its_domain(
            lanes in prop::array::uniform4(-(1_i64 << 51) + 1..1 << 51),
            divisor in 1_i64..1 << 51,
            shift in 0_u32..51,
        ) {
            let divisor = (divisor >> shift).max(1);
            let expected = lanes.map(|lane| lane.div_euclid(divisor));
            prop_assert_eq!(I64x4(lanes).div_euclid(divisor).0, expected);
        }
    }
}
