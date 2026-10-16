use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

use serde::{Deserialize, Serialize};

use crate::num::{Num, SinCos};
use crate::vec3::step_moves::StepMoves;

#[cfg(feature = "internals")]
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
    pub const ZERO: Self = Self::new(Num::ZERO, Num::ZERO, Num::ZERO);

    pub const fn new(x: Num, y: Num, z: Num) -> Self {
        Self { x, y, z }
    }

    pub const fn checked_add(self, rhs: Self) -> Option<Self> {
        Self::from_parts(
            self.x.checked_add(rhs.x),
            self.y.checked_add(rhs.y),
            self.z.checked_add(rhs.z),
        )
    }

    pub const fn checked_sub(self, rhs: Self) -> Option<Self> {
        Self::from_parts(
            self.x.checked_sub(rhs.x),
            self.y.checked_sub(rhs.y),
            self.z.checked_sub(rhs.z),
        )
    }

    pub const fn checked_neg(self) -> Option<Self> {
        Self::from_parts(
            self.x.checked_neg(),
            self.y.checked_neg(),
            self.z.checked_neg(),
        )
    }

    pub const fn checked_mul(self, factor: Num) -> Option<Self> {
        Self::from_parts(
            self.x.checked_mul(factor),
            self.y.checked_mul(factor),
            self.z.checked_mul(factor),
        )
    }

    pub fn checked_div(self, divisor: Num) -> Option<Self> {
        Self::from_parts(
            self.x.checked_div(divisor),
            self.y.checked_div(divisor),
            self.z.checked_div(divisor),
        )
    }

    /// The dot product, rounded once from the exact sum of products.
    pub const fn checked_dot(self, rhs: Self) -> Option<Num> {
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
    pub const fn dot(self, rhs: Self) -> Num {
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

    pub fn checked_distance(self, other: Self) -> Option<Num> {
        other.checked_sub(self)?.checked_length()
    }

    #[must_use]
    pub fn distance(self, other: Self) -> Num {
        self.checked_distance(other)
            .expect("Vec3 overflow in distance")
    }

    /// Whether `other` is at most `radius` away, decided exactly and without a square root.
    pub const fn within(self, other: Self, radius: Num) -> bool {
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

    /// The vector with its height dropped: its point on the ground plane.
    #[must_use]
    pub const fn ground(self) -> Self {
        Self::new(self.x, Num::ZERO, self.z)
    }

    /// The offset from here to `to` on the ground plane, heights dropped.
    #[must_use]
    pub const fn ground_offset(self, to: Self) -> Self {
        to.ground()
            .checked_sub(self.ground())
            .expect("Vec3 overflow in ground_offset")
    }

    /// Whether `other` is at most `radius` away on the ground plane, heights dropped, decided
    /// exactly: the range test of a planar map.
    pub const fn within_ground(self, other: Self, radius: Num) -> bool {
        self.ground().within(other.ground(), radius)
    }

    /// The unit vector in this direction, each component rounded once; `None` for the zero
    /// vector.
    pub fn normalized(self) -> Option<Self> {
        let length = self.checked_length()?.to_bits().cast_unsigned();
        if length == 0 {
            return None;
        }
        // One float quotient and three multiplies estimate the three quotients, and
        // `unit_component` corrects each exactly, so the result does not depend on the float.
        let scale = unit_scale(length);
        Some(Self::new(
            unit_component(self.x, length, scale),
            unit_component(self.y, length, scale),
            unit_component(self.z, length, scale),
        ))
    }

    /// The unit vector from `self` towards `other`; `None` when they coincide.
    pub fn direction_to(self, other: Self) -> Option<Self> {
        other.checked_sub(self)?.normalized()
    }

    /// The point `step` along the way to `target`, or `target` when it is at most `step` away.
    /// Each component moves by `offset · step / distance`, rounded once, so it never passes the
    /// target's. `None` when the offset or the distance does not fit a `Num`.
    pub fn checked_step_toward(self, target: Self, step: Num) -> Option<Self> {
        debug_assert!(step >= Num::ZERO, "Vec3::step_toward with a negative step");
        if self.within(target, step) {
            return Some(target);
        }
        let offset = target.checked_sub(self)?;
        // The exact distance is above `step`, so the rounded one is at least `step`: every
        // ratio below is at most 1, and each move at most its offset.
        let distance = offset.checked_length()?.to_bits();
        let moves = StepMoves::new(step.to_bits(), distance).of([offset.x, offset.y, offset.z]);
        // A move ends between the start and the target, so its sum cannot wrap, and the wrapping
        // step skips the check a release build makes.
        let advance = |from: Num, moved: Num| {
            debug_assert!(from.checked_add(moved).is_some());
            Num::from_bits(from.to_bits().wrapping_add(moved.to_bits()))
        };
        Some(Self::new(
            advance(self.x, moves[0]),
            advance(self.y, moves[1]),
            advance(self.z, moves[2]),
        ))
    }

    #[must_use]
    pub fn step_toward(self, target: Self, step: Num) -> Self {
        self.checked_step_toward(target, step)
            .expect("Vec3 overflow in step_toward")
    }

    /// Turned about the y axis by the angle of `turn`, counter-clockwise seen from above; each
    /// component rounded once from the exact sum of products.
    pub const fn checked_rotated_y(self, turn: SinCos) -> Option<Self> {
        let (x, z) = (self.x.to_bits() as i128, self.z.to_bits() as i128);
        let (sin, cos) = (turn.sin.to_bits() as i128, turn.cos.to_bits() as i128);
        match (
            Num::from_raw_products(x * cos + z * sin),
            Num::from_raw_products(z * cos - x * sin),
        ) {
            (Some(x), Some(z)) => Some(Self::new(x, self.y, z)),
            _ => None,
        }
    }

    #[must_use]
    pub const fn rotated_y(self, turn: SinCos) -> Self {
        match self.checked_rotated_y(turn) {
            Some(turned) => turned,
            None => panic!("Vec3 overflow in rotated_y"),
        }
    }

    /// A vector when all three components exist.
    const fn from_parts(x: Option<Num>, y: Option<Num>, z: Option<Num>) -> Option<Self> {
        match (x, y, z) {
            (Some(x), Some(y), Some(z)) => Some(Self::new(x, y, z)),
            _ => None,
        }
    }

    /// Its axes in bits, `[x, y, z]`, widened for the products of a `Direction`.
    pub const fn to_bits(self) -> [i128; 3] {
        [
            self.x.to_bits() as i128,
            self.y.to_bits() as i128,
            self.z.to_bits() as i128,
        ]
    }

    /// The squared length in raw units, 2⁻⁴⁸ each, exact: each square is at most 2¹²⁶, so three
    /// fit a `u128`. It orders distances exactly, where rounded lengths can tie.
    pub const fn length_squared_bits(self) -> u128 {
        let x = self.x.to_bits().unsigned_abs() as u128;
        let y = self.y.to_bits().unsigned_abs() as u128;
        let z = self.z.to_bits().unsigned_abs() as u128;
        // Below 3 · 2¹²⁶, the sum cannot wrap, and the wrapping steps skip the checks a release
        // build makes.
        x.wrapping_mul(x)
            .wrapping_add(y.wrapping_mul(y))
            .wrapping_add(z.wrapping_mul(z))
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
/// quotient is at most 2²⁴. The estimate rounds four times, twice in `scale`, once in the
/// magnitude's conversion and once in the fused multiply-add, by at most 2⁻⁵³ of itself each, so
/// it lies within 2⁻²⁶ of the quotient, and lifted by 2⁻²⁰ it lies above it and within 2⁻¹⁹: its
/// floor is the quotient's floor or one above it, the second only
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
    // The quotient is at most 2²⁴ + 1 and the rest within ±2⁶³, so no step wraps, and the
    // wrapping steps skip the checks a release build makes.
    if rest < 0 {
        quotient = quotient.wrapping_sub(1);
        rest = rest.wrapping_add(length.cast_signed());
    }
    // The rounding and the sign are as random as the components, so both are arithmetic, which
    // takes no branch to mispredict.
    let twice_rest = rest.cast_unsigned() << 1;
    #[expect(
        clippy::needless_bitwise_bool,
        reason = "the lazy operators branch on a random rounding"
    )]
    let up = (twice_rest > length) | ((twice_rest == length) & (quotient & 1 == 1));
    let quotient = quotient.wrapping_add(u64::from(up)).cast_signed();
    // All ones for a negative component, so the xor and the subtraction negate.
    let sign = component.to_bits() >> 63;
    Num::from_bits((quotient ^ sign).wrapping_sub(sign))
}

impl Add for Vec3 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        self.checked_add(rhs).expect("Vec3 overflow in +")
    }
}

impl Sub for Vec3 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        self.checked_sub(rhs).expect("Vec3 overflow in -")
    }
}

impl Neg for Vec3 {
    type Output = Self;

    fn neg(self) -> Self {
        self.checked_neg().expect("Vec3 overflow in unary -")
    }
}

impl Mul<Num> for Vec3 {
    type Output = Self;

    fn mul(self, factor: Num) -> Self {
        self.checked_mul(factor).expect("Vec3 overflow in *")
    }
}

impl Div<Num> for Vec3 {
    type Output = Self;

    fn div(self, divisor: Num) -> Self {
        self.checked_div(divisor)
            .expect("Vec3 division by zero or overflow in /")
    }
}

impl AddAssign for Vec3 {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl SubAssign for Vec3 {
    fn sub_assign(&mut self, rhs: Self) {
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

/// Exact, as each element's conversion to a `Num` is.
impl<X: Into<Num>, Y: Into<Num>, Z: Into<Num>> From<(X, Y, Z)> for Vec3 {
    fn from((x, y, z): (X, Y, Z)) -> Self {
        Self::new(x.into(), y.into(), z.into())
    }
}

/// Exact, as each element's conversion to a `Num` is.
impl<T: Into<Num>> From<[T; 3]> for Vec3 {
    fn from([x, y, z]: [T; 3]) -> Self {
        Self::new(x.into(), y.into(), z.into())
    }
}

#[cfg(test)]
mod tests;
