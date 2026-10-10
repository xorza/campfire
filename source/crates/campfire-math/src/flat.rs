use std::ops::{Add, Index, Mul, Neg, Sub};

use crate::num::Num;
use crate::vec3::Vec3;

/// A point or a vector of the ground plane, `[x, z]`, in `i128`s at a scale its caller picks, a
/// `Num`'s bits or finer: sums, differences and products of coordinates that pass a `Num` stay
/// exact. Each operation is the `i128` one, which panics on overflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flat([i128; 2]);

impl Flat {
    pub const fn new(x: i128, z: i128) -> Flat {
        Flat([x, z])
    }

    pub const fn from_array(axes: [i128; 2]) -> Flat {
        Flat(axes)
    }

    pub const fn to_array(self) -> [i128; 2] {
        self.0
    }

    /// `[x, z]` of `nums`, in their bits.
    pub const fn from_nums(nums: [Num; 2]) -> Flat {
        Flat([nums[0].to_bits() as i128, nums[1].to_bits() as i128])
    }

    /// Where `at` stands on the ground plane, in a `Num`'s bits.
    pub const fn ground(at: Vec3) -> Flat {
        Flat::from_nums([at.x, at.z])
    }

    pub const fn dot(self, rhs: Flat) -> i128 {
        self.0[0] * rhs.0[0] + self.0[1] * rhs.0[1]
    }

    /// Positive when `rhs` turns counterclockwise from `self`, seen from above.
    pub const fn cross(self, rhs: Flat) -> i128 {
        self.0[0] * rhs.0[1] - self.0[1] * rhs.0[0]
    }

    pub const fn length_squared(self) -> u128 {
        self.dot(self).unsigned_abs()
    }

    /// `f` of each axis.
    #[must_use]
    pub fn map(self, f: impl FnMut(i128) -> i128) -> Flat {
        Flat(self.0.map(f))
    }
}

impl Index<usize> for Flat {
    type Output = i128;

    fn index(&self, axis: usize) -> &i128 {
        &self.0[axis]
    }
}

impl Add for Flat {
    type Output = Flat;

    fn add(self, rhs: Flat) -> Flat {
        Flat([self.0[0] + rhs.0[0], self.0[1] + rhs.0[1]])
    }
}

impl Sub for Flat {
    type Output = Flat;

    fn sub(self, rhs: Flat) -> Flat {
        Flat([self.0[0] - rhs.0[0], self.0[1] - rhs.0[1]])
    }
}

impl Neg for Flat {
    type Output = Flat;

    fn neg(self) -> Flat {
        Flat([-self.0[0], -self.0[1]])
    }
}

impl Mul<i128> for Flat {
    type Output = Flat;

    fn mul(self, factor: i128) -> Flat {
        Flat([self.0[0] * factor, self.0[1] * factor])
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
        // Products past an i64 stay exact: (2⁶³ − 1)² + 1 and (2⁶³ − 1) · (2⁶³ − 1) − 0.
        let wide = Flat::new(i128::from(i64::MAX), 1);
        let square = i128::from(i64::MAX) * i128::from(i64::MAX);
        assert_eq!(wide.dot(wide), square + 1);
        assert_eq!(wide.cross(Flat::new(0, i128::from(i64::MAX))), square);
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
        let _sum = Flat::new(i128::MAX, 0) + Flat::new(1, 0);
    }
}
