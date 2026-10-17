use std::ops::{Add, Index, Mul, Neg, Sub};

use crate::num::Num;
use crate::vec3::Vec3;

/// A point or a vector of the ground plane, `[x, z]`, at a scale its caller picks, a `Num`'s bits
/// or finer, each coordinate within `i64`: sums and differences stay exact in `i64`, and each
/// product of two, as `dot` and `cross` take it, in `i128`, where a product of two `i64`s needs
/// one widening multiply and no overflow check. Each operation panics on overflow, as does a
/// coordinate past `i64`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flat([i64; 2]);

impl Flat {
    pub const fn new(x: i128, z: i128) -> Self {
        Self([Self::narrow(x), Self::narrow(z)])
    }

    pub const fn from_array(axes: [i128; 2]) -> Self {
        Self::new(axes[0], axes[1])
    }

    pub const fn to_array(self) -> [i128; 2] {
        [self.0[0] as i128, self.0[1] as i128]
    }

    /// `[x, z]` of `nums`, in their bits.
    pub const fn from_nums(nums: [Num; 2]) -> Self {
        Self([nums[0].to_bits(), nums[1].to_bits()])
    }

    /// Where `point` stands on the ground plane, in a `Num`'s bits.
    pub const fn ground(point: Vec3) -> Self {
        Self::from_nums([point.x, point.z])
    }

    /// Within `i128` but for two vectors of `i64::MIN` on both axes, whose 2¹²⁷ panics, as a
    /// release build's `+` would wrap.
    pub const fn dot(self, rhs: Self) -> i128 {
        let x = Self::product(self.0[0], rhs.0[0]);
        x.checked_add(Self::product(self.0[1], rhs.0[1]))
            .expect("a dot product within i128")
    }

    /// Positive when `rhs` turns counterclockwise from `self`, seen from above.
    pub const fn cross(self, rhs: Self) -> i128 {
        Self::product(self.0[0], rhs.0[1]) - Self::product(self.0[1], rhs.0[0])
    }

    /// `x² + z²`, exact: each square is at most 2¹²⁶, so their sum, 2¹²⁷ at `i64::MIN` on both
    /// axes, fits a `u128`, where `dot`'s `i128` would wrap.
    pub const fn length_squared(self) -> u128 {
        let x = self.0[0].unsigned_abs() as u128;
        let z = self.0[1].unsigned_abs() as u128;
        // Below 2¹²⁸, the sum cannot wrap, and the wrapping steps skip the checks a release
        // build makes.
        x.wrapping_mul(x).wrapping_add(z.wrapping_mul(z))
    }

    /// `f` of each axis.
    #[must_use]
    pub fn map(self, f: impl FnMut(i128) -> i128) -> Self {
        Self::from_array(self.to_array().map(f))
    }

    #[expect(clippy::cast_possible_truncation, reason = "checked within i64")]
    const fn narrow(axis: i128) -> i64 {
        assert!(
            i64::MIN as i128 <= axis && axis <= i64::MAX as i128,
            "a ground coordinate within i64"
        );
        axis as i64
    }

    /// `a · b`, exact: within ±2¹²⁶, so it never overflows.
    const fn product(a: i64, b: i64) -> i128 {
        (a as i128).wrapping_mul(b as i128)
    }
}

impl Index<usize> for Flat {
    type Output = i64;

    fn index(&self, axis: usize) -> &i64 {
        &self.0[axis]
    }
}

impl Add for Flat {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self([self.0[0] + rhs.0[0], self.0[1] + rhs.0[1]])
    }
}

impl Sub for Flat {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        Self([self.0[0] - rhs.0[0], self.0[1] - rhs.0[1]])
    }
}

impl Neg for Flat {
    type Output = Self;

    fn neg(self) -> Self {
        Self([-self.0[0], -self.0[1]])
    }
}

impl Mul<i64> for Flat {
    type Output = Self;

    fn mul(self, factor: i64) -> Self {
        Self([self.0[0] * factor, self.0[1] * factor])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_operation_is_exact_on_both_axes() {
        let (a, b) = (Flat::new(3, -4), Flat::new(5, 2));
        assert_eq!(a + b, Flat::new(8, -2));
        assert_eq!(a - b, Flat::new(-2, -6));
        assert_eq!(-a, Flat::new(-3, 4));
        assert_eq!(a * 3, Flat::new(9, -12));
        assert_eq!(a.map(|axis| axis * axis), Flat::new(9, 16));
        assert_eq!((a[0], a[1]), (3, -4));
        assert_eq!(a.to_array(), [3, -4]);
        assert_eq!(Flat::from_array([3, -4]), a);
        // 3·5 + (−4)·2 = 7; 3·2 − (−4)·5 = 26, so b turns counterclockwise from a, and a
        // clockwise from b; 3² + 4² = 25.
        assert_eq!(a.dot(b), 7);
        assert_eq!((a.cross(b), b.cross(a)), (26, -26));
        assert_eq!(a.length_squared(), 25);
        assert_eq!(Flat::new(0, 0).length_squared(), 0);
        // The longest vector, (−2⁶³)² + (−2⁶³)² = 2¹²⁷, one past what an `i128` holds.
        let longest = Flat::new(i128::from(i64::MIN), i128::from(i64::MIN));
        assert_eq!(longest.length_squared(), 1 << 127);
        // Products past an i64 stay exact: (2⁶³ − 1)² + 1 and (2⁶³ − 1) · (2⁶³ − 1) − 0; and the
        // widest cross, (−2⁶³)(−2⁶³) − (−2⁶³)(2⁶³ − 1) = 2¹²⁷ − 2⁶³.
        let wide = Flat::new(i128::from(i64::MAX), 1);
        let square = i128::from(i64::MAX) * i128::from(i64::MAX);
        assert_eq!(wide.dot(wide), square + 1);
        assert_eq!(wide.cross(Flat::new(0, i128::from(i64::MAX))), square);
        let (min, max) = (i128::from(i64::MIN), i128::from(i64::MAX));
        assert_eq!(
            Flat::new(min, min).cross(Flat::new(max, min)),
            i128::MAX - (1 << 63) + 1
        );
        // A vector's ground: its x and z in bits, its height dropped.
        let at = Vec3::new(Num::ONE, Num::int(7), -Num::HALF);
        assert_eq!(Flat::ground(at), Flat::new(1 << 24, -(1 << 23)));
        assert_eq!(
            Flat::from_nums([Num::MIN, Num::MAX]).to_array(),
            [i128::from(i64::MIN), i128::from(i64::MAX)]
        );
    }

    #[test]
    #[should_panic(expected = "overflow")]
    fn an_overflow_panics() {
        let _sum = Flat::new(i128::from(i64::MAX), 0) + Flat::new(1, 0);
    }

    #[test]
    #[should_panic(expected = "a dot product within i128")]
    fn the_dot_product_of_the_two_longest_vectors_panics() {
        let longest = Flat::new(i128::from(i64::MIN), i128::from(i64::MIN));
        let _dot = longest.dot(longest);
    }

    #[test]
    #[should_panic(expected = "a ground coordinate within i64")]
    fn a_coordinate_past_i64_panics() {
        let _past = Flat::new(i128::from(i64::MAX) + 1, 0);
    }
}
