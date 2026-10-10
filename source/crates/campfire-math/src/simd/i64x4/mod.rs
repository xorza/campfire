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
mod tests;
