use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

use serde::{Deserialize, Serialize};

use crate::num::{Num, SinCos};
use crate::vec3::step_moves::StepMoves;

#[cfg(feature = "bench")]
pub(crate) mod bench;
mod step_moves;

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

    pub const fn checked_mul(self, factor: Num) -> Option<Vec3> {
        Vec3::from_parts(
            self.x.checked_mul(factor),
            self.y.checked_mul(factor),
            self.z.checked_mul(factor),
        )
    }

    pub fn checked_div(self, divisor: Num) -> Option<Vec3> {
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
        // One float quotient and three multiplies estimate the three quotients, and
        // `unit_component` corrects each exactly, so the result does not depend on the float.
        let scale = unit_scale(length);
        Some(Vec3::new(
            unit_component(self.x, length, scale),
            unit_component(self.y, length, scale),
            unit_component(self.z, length, scale),
        ))
    }

    /// The unit vector from `self` towards `other`; `None` when they coincide.
    pub fn direction_to(self, other: Vec3) -> Option<Vec3> {
        other.checked_sub(self)?.normalized()
    }

    /// The point `step` along the way to `target`, or `target` when it is at most `step` away.
    /// Each component moves by `offset · step / distance`, rounded once, so it never passes the
    /// target's. `None` when the offset or the distance does not fit a `Num`.
    pub fn checked_step_toward(self, target: Vec3, step: Num) -> Option<Vec3> {
        debug_assert!(step >= Num::ZERO, "Vec3::step_toward with a negative step");
        if self.within(target, step) {
            return Some(target);
        }
        let offset = target.checked_sub(self)?;
        // The exact distance is above `step`, so the rounded one is at least `step`: every
        // ratio below is at most 1, and each move at most its offset.
        let distance = offset.checked_length()?.to_bits();
        let moves = StepMoves::new(step.to_bits(), distance).of([offset.x, offset.y, offset.z]);
        let advance = |from: Num, moved: Num| {
            from.checked_add(moved)
                .expect("a move ends between the start and the target")
        };
        Some(Vec3::new(
            advance(self.x, moves[0]),
            advance(self.y, moves[1]),
            advance(self.z, moves[2]),
        ))
    }

    #[must_use]
    pub fn step_toward(self, target: Vec3, step: Num) -> Vec3 {
        self.checked_step_toward(target, step)
            .expect("Vec3 overflow in step_toward")
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

    /// The squared length in raw units, 2⁻⁴⁸ each, exact: each square is at most 2¹²⁶, so three
    /// fit a `u128`. It orders distances exactly, where rounded lengths can tie.
    pub const fn length_squared_bits(self) -> u128 {
        let x = self.x.to_bits().unsigned_abs() as u128;
        let y = self.y.to_bits().unsigned_abs() as u128;
        let z = self.z.to_bits().unsigned_abs() as u128;
        x.wrapping_mul(x) + y.wrapping_mul(y) + z.wrapping_mul(z)
    }
}

/// 2²⁴, exact in f64.
const TWO_POW_24: f64 = 16_777_216.0;
/// 2⁻²⁰, exact in f64: what lifts a quotient's estimate past its error.
const TWO_POW_MINUS_20: f64 = 1.0 / 1_048_576.0;

/// 2²⁴ / `length`, rounded twice, for a `length` from 1 to below 2⁶³: within 2⁻⁵² of it.
#[expect(
    clippy::cast_precision_loss,
    clippy::float_arithmetic,
    reason = "an estimate that `unit_component` corrects exactly"
)]
fn unit_scale(length: u64) -> f64 {
    debug_assert!(0 < length && length < 1 << 63);
    TWO_POW_24 / length.cast_signed() as f64
}

/// `component / length` rounded to nearest, ties to even, from `scale`, `unit_scale(length)`.
/// |component| ≤ length, because the length is the nearest root of the squared sum, so the
/// quotient is at most 2²⁴. The estimate `|component| · scale` rounds three times, by at most
/// 2⁻⁵³ of itself each, so it lies within 2⁻²⁷ of the quotient, and lifted by 2⁻²⁰ it lies
/// above it and within 2⁻¹⁹: its floor is the quotient's floor or one above it, the second only
/// for a quotient within 2⁻¹⁹ below an integer, which one rare step corrects. A whole quotient,
/// as an axis's component gives, takes no step.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "an estimate of at most 2²⁴ + 1, which the integer steps correct"
)]
fn unit_component(component: Num, length: u64, scale: f64) -> Num {
    let magnitude = component.to_bits().unsigned_abs();
    let estimate = (magnitude.cast_signed() as f64).mul_add(scale, TWO_POW_MINUS_20);
    let mut quotient = estimate as i64 as u64;
    // The rest `magnitude · 2²⁴ − quotient · length` lies from −length to below length, within
    // 2⁶³, so its low 64 bits, wrapped, are the rest, and every step runs on `u64`.
    let mut rest = (magnitude << Num::FRAC_BITS)
        .wrapping_sub(quotient.wrapping_mul(length))
        .cast_signed();
    if rest < 0 {
        quotient -= 1;
        rest += length.cast_signed();
    }
    // The rounding and the sign are as random as the components, so both are arithmetic, which
    // takes no branch to mispredict.
    let twice_rest = rest.cast_unsigned() << 1;
    #[expect(
        clippy::needless_bitwise_bool,
        reason = "the lazy operators branch on a random rounding"
    )]
    let up = (twice_rest > length) | ((twice_rest == length) & (quotient & 1 == 1));
    let quotient = (quotient + u64::from(up)).cast_signed();
    // All ones for a negative component, so the xor and the subtraction negate.
    let sign = component.to_bits() >> 63;
    Num::from_bits((quotient ^ sign) - sign)
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
        self.checked_mul(factor).expect("Vec3 overflow in *")
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

impl MulAssign<Num> for Vec3 {
    fn mul_assign(&mut self, factor: Num) {
        *self = *self * factor;
    }
}

impl DivAssign<Num> for Vec3 {
    fn div_assign(&mut self, divisor: Num) {
        *self = *self / divisor;
    }
}

#[cfg(test)]
mod tests;
