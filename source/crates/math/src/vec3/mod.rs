use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

use serde::{Deserialize, Serialize};

use crate::num::{Num, SinCos};

#[cfg(feature = "bench")]
pub(crate) mod bench;

/// A 3D vector of `Num`s; the ground plane is x and z, height is y.
///
/// Operators panic on overflow; the `checked_*` methods return `None` instead.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Vec3 {
    pub x: Num,
    pub y: Num,
    pub z: Num,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3::new(Num::ZERO, Num::ZERO, Num::ZERO);

    pub const fn new(x: Num, y: Num, z: Num) -> Vec3 {
        Vec3 { x, y, z }
    }

    pub const fn checked_add(self, rhs: Vec3) -> Option<Vec3> {
        Vec3::from_parts(
            self.x.checked_add(rhs.x),
            self.y.checked_add(rhs.y),
            self.z.checked_add(rhs.z),
        )
    }

    pub const fn checked_sub(self, rhs: Vec3) -> Option<Vec3> {
        Vec3::from_parts(
            self.x.checked_sub(rhs.x),
            self.y.checked_sub(rhs.y),
            self.z.checked_sub(rhs.z),
        )
    }

    pub const fn checked_neg(self) -> Option<Vec3> {
        Vec3::from_parts(
            self.x.checked_neg(),
            self.y.checked_neg(),
            self.z.checked_neg(),
        )
    }

    pub const fn checked_scale(self, factor: Num) -> Option<Vec3> {
        Vec3::from_parts(
            self.x.checked_mul(factor),
            self.y.checked_mul(factor),
            self.z.checked_mul(factor),
        )
    }

    pub const fn checked_div(self, divisor: Num) -> Option<Vec3> {
        Vec3::from_parts(
            self.x.checked_div(divisor),
            self.y.checked_div(divisor),
            self.z.checked_div(divisor),
        )
    }

    /// The dot product, rounded once from the exact sum of products.
    pub const fn checked_dot(self, rhs: Vec3) -> Option<Num> {
        let x = self.x.to_bits() as i128 * rhs.x.to_bits() as i128;
        let y = self.y.to_bits() as i128 * rhs.y.to_bits() as i128;
        let z = self.z.to_bits() as i128 * rhs.z.to_bits() as i128;
        let Some(xy) = x.checked_add(y) else {
            return None;
        };
        let Some(sum) = xy.checked_add(z) else {
            return None;
        };
        Num::from_raw_products(sum)
    }

    #[must_use]
    pub const fn dot(self, rhs: Vec3) -> Num {
        match self.checked_dot(rhs) {
            Some(dot) => dot,
            None => panic!("Vec3 overflow in dot"),
        }
    }

    /// The exact length, rounded once; `None` when it does not fit a `Num`.
    pub fn checked_length(self) -> Option<Num> {
        Num::from_root_of_bits(self.length_squared_bits())
    }

    #[must_use]
    pub fn length(self) -> Num {
        self.checked_length().expect("Vec3 overflow in length")
    }

    pub fn checked_distance(self, other: Vec3) -> Option<Num> {
        other.checked_sub(self)?.checked_length()
    }

    #[must_use]
    pub fn distance(self, other: Vec3) -> Num {
        self.checked_distance(other)
            .expect("Vec3 overflow in distance")
    }

    /// Whether `other` is at most `radius` away, decided exactly and without a square root.
    pub const fn within(self, other: Vec3, radius: Num) -> bool {
        if radius.to_bits() < 0 {
            return false;
        }
        let Some(offset) = other.checked_sub(self) else {
            // A component difference beyond `Num` is farther than any radius.
            return false;
        };
        let reach = radius.to_bits().cast_unsigned() as u128;
        offset.length_squared_bits() <= reach * reach
    }

    /// The unit vector in this direction, each component rounded once; `None` for the zero
    /// vector.
    pub fn normalized(self) -> Option<Vec3> {
        let length = self.checked_length()?.to_bits().cast_unsigned();
        if length == 0 {
            return None;
        }
        // One reciprocal and three multiplies in place of three divisions; `unit_component`
        // corrects each quotient exactly, so the result is the same.
        let reciprocal = (1 << (Num::FRAC_BITS + RECIPROCAL_BITS)) / u128::from(length);
        Some(Vec3::new(
            unit_component(self.x, length, reciprocal),
            unit_component(self.y, length, reciprocal),
            unit_component(self.z, length, reciprocal),
        ))
    }

    /// The unit vector from `self` towards `other`; `None` when they coincide.
    pub fn direction_to(self, other: Vec3) -> Option<Vec3> {
        other.checked_sub(self)?.normalized()
    }

    /// Turned about the y axis by the angle of `turn`, counter-clockwise seen from above; each
    /// component rounded once from the exact sum of products.
    pub const fn checked_rotated_y(self, turn: SinCos) -> Option<Vec3> {
        let (x, z) = (self.x.to_bits() as i128, self.z.to_bits() as i128);
        let (sin, cos) = (turn.sin.to_bits() as i128, turn.cos.to_bits() as i128);
        match (
            Num::from_raw_products(x * cos + z * sin),
            Num::from_raw_products(z * cos - x * sin),
        ) {
            (Some(x), Some(z)) => Some(Vec3::new(x, self.y, z)),
            _ => None,
        }
    }

    #[must_use]
    pub const fn rotated_y(self, turn: SinCos) -> Vec3 {
        match self.checked_rotated_y(turn) {
            Some(turned) => turned,
            None => panic!("Vec3 overflow in rotated_y"),
        }
    }

    /// A vector when all three components exist.
    const fn from_parts(x: Option<Num>, y: Option<Num>, z: Option<Num>) -> Option<Vec3> {
        match (x, y, z) {
            (Some(x), Some(y), Some(z)) => Some(Vec3::new(x, y, z)),
            _ => None,
        }
    }

    /// The squared length in raw units, exact: each square is at most 2¹²⁶, so three fit a
    /// `u128`.
    const fn length_squared_bits(self) -> u128 {
        let x = self.x.to_bits().unsigned_abs() as u128;
        let y = self.y.to_bits().unsigned_abs() as u128;
        let z = self.z.to_bits().unsigned_abs() as u128;
        x.wrapping_mul(x) + y.wrapping_mul(y) + z.wrapping_mul(z)
    }
}

/// Fractional bits of the reciprocal in `normalized`.
const RECIPROCAL_BITS: u32 = 62;

/// `component / length` rounded to nearest, ties to even, from `reciprocal` =
/// ⌊2⁸⁶ / length⌋ (`length` in raw units).
const fn unit_component(component: Num, length: u64, reciprocal: u128) -> Num {
    // |component| ≤ length, because the length is the nearest root of the squared sum, so the
    // product stays below 2⁸⁷ and the quotient at most 2²⁴.
    let magnitude = component.to_bits().unsigned_abs() as u128;
    let divisor = length as u128;
    let numerator = magnitude << Num::FRAC_BITS;
    // The reciprocal is short by less than one unit, which leaves the estimate at most two
    // below the true quotient. With these bounds no step below can overflow.
    let mut quotient = magnitude.wrapping_mul(reciprocal) >> RECIPROCAL_BITS;
    let mut rest = numerator.wrapping_sub(quotient.wrapping_mul(divisor));
    while rest >= divisor {
        quotient = quotient.wrapping_add(1);
        rest = rest.wrapping_sub(divisor);
    }
    let twice_rest = rest << 1;
    if twice_rest > divisor || (twice_rest == divisor && quotient & 1 == 1) {
        quotient = quotient.wrapping_add(1);
    }
    let quotient = quotient.cast_signed();
    Num::from_wide_bits(if component.to_bits() < 0 {
        -quotient
    } else {
        quotient
    })
}

impl Add for Vec3 {
    type Output = Vec3;

    fn add(self, rhs: Vec3) -> Vec3 {
        self.checked_add(rhs).expect("Vec3 overflow in +")
    }
}

impl Sub for Vec3 {
    type Output = Vec3;

    fn sub(self, rhs: Vec3) -> Vec3 {
        self.checked_sub(rhs).expect("Vec3 overflow in -")
    }
}

impl Neg for Vec3 {
    type Output = Vec3;

    fn neg(self) -> Vec3 {
        self.checked_neg().expect("Vec3 overflow in unary -")
    }
}

impl Mul<Num> for Vec3 {
    type Output = Vec3;

    fn mul(self, factor: Num) -> Vec3 {
        self.checked_scale(factor).expect("Vec3 overflow in *")
    }
}

impl Div<Num> for Vec3 {
    type Output = Vec3;

    fn div(self, divisor: Num) -> Vec3 {
        self.checked_div(divisor)
            .expect("Vec3 division by zero or overflow in /")
    }
}

impl AddAssign for Vec3 {
    fn add_assign(&mut self, rhs: Vec3) {
        *self = *self + rhs;
    }
}

impl SubAssign for Vec3 {
    fn sub_assign(&mut self, rhs: Vec3) {
        *self = *self - rhs;
    }
}

#[cfg(test)]
mod tests;
