use std::cmp::Ordering;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

use serde::{Deserialize, Serialize};

use crate::coordinate;
use crate::direction::Direction;
use crate::num::{Num, SinCos};
use crate::rounding::Rounding;

#[cfg(feature = "internals")]
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
    pub const ZERO: Self = Self::new(Num::ZERO, Num::ZERO, Num::ZERO);

    /// 2²⁰ m: the bound of every coordinate of a place, which a map's bounds lie within, so exact
    /// squared distances between two places fit a `u128` with room to spare.
    pub const BOUND: Num = Num::from_bits(1 << coordinate::BOUND_BITS);

    pub const fn new(x: Num, y: Num, z: Num) -> Self {
        Self { x, y, z }
    }

    pub const fn checked_add(self, rhs: Self) -> Option<Self> {
        Self::from_axes(
            self.x.checked_add(rhs.x),
            self.y.checked_add(rhs.y),
            self.z.checked_add(rhs.z),
        )
    }

    pub const fn checked_sub(self, rhs: Self) -> Option<Self> {
        Self::from_axes(
            self.x.checked_sub(rhs.x),
            self.y.checked_sub(rhs.y),
            self.z.checked_sub(rhs.z),
        )
    }

    pub const fn checked_neg(self) -> Option<Self> {
        Self::from_axes(
            self.x.checked_neg(),
            self.y.checked_neg(),
            self.z.checked_neg(),
        )
    }

    pub const fn checked_mul(self, factor: Num) -> Option<Self> {
        Self::from_axes(
            self.x.checked_mul(factor, Rounding::NearestEven),
            self.y.checked_mul(factor, Rounding::NearestEven),
            self.z.checked_mul(factor, Rounding::NearestEven),
        )
    }

    pub fn checked_div(self, divisor: Num) -> Option<Self> {
        Self::from_axes(
            self.x.checked_div(divisor, Rounding::NearestEven),
            self.y.checked_div(divisor, Rounding::NearestEven),
            self.z.checked_div(divisor, Rounding::NearestEven),
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

    /// How the distance to `other` lies against `radius`, decided exactly and without a square
    /// root; past it for a negative radius.
    pub const fn distance_order(self, other: Self, radius: Num) -> Ordering {
        if radius.to_bits() < 0 {
            return Ordering::Greater;
        }
        let Some(offset) = other.checked_sub(self) else {
            // A component difference beyond `Num` is farther than any radius.
            return Ordering::Greater;
        };
        let reach = radius.to_bits().cast_unsigned() as u128;
        let (square, reach) = (offset.length_squared_bits(), reach * reach);
        if square < reach {
            Ordering::Less
        } else if square == reach {
            Ordering::Equal
        } else {
            Ordering::Greater
        }
    }

    /// Whether `other` is at most `radius` away, decided exactly and without a square root.
    pub const fn within(self, other: Self, radius: Num) -> bool {
        !matches!(self.distance_order(other, radius), Ordering::Greater)
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

    /// Whether each of its coordinates lies within ±`BOUND`, as a place's does.
    pub const fn within_bound(self) -> bool {
        let bound = Self::BOUND.to_bits().unsigned_abs();
        self.x.to_bits().unsigned_abs() <= bound
            && self.y.to_bits().unsigned_abs() <= bound
            && self.z.to_bits().unsigned_abs() <= bound
    }

    /// Whether `other` is at most `radius` away on the ground plane, heights dropped, decided
    /// exactly: the range test of a planar map.
    pub const fn within_ground(self, other: Self, radius: Num) -> bool {
        self.ground().within(other.ground(), radius)
    }

    /// Its components' bits, widened; `None` for one past ±2⁴⁷ bits, 2²³ m, beyond any offset of
    /// two places.
    fn wide_axes(self) -> Option<[i128; 3]> {
        let vector = [self.x, self.y, self.z].map(|component| i128::from(component.to_bits()));
        let narrow =
            |component: &i128| component.unsigned_abs() < coordinate::WIDEST.unsigned_abs();
        vector.iter().all(narrow).then_some(vector)
    }

    /// The unit vector in this direction, each component the true unit's rounded once to the
    /// nearest 2⁻²⁴, so within 2⁻²⁵ of it, whatever the vector's length; `None` for the zero
    /// vector, and for one with a component past ±2⁴⁷ bits, 2²³ m, beyond any offset of two
    /// places.
    pub fn normalized(self) -> Option<Self> {
        let vector = self.wide_axes()?;
        let direction = Direction::of(vector)?;
        let square = self.length_squared_bits();
        let [x, y, z] = direction.unit();
        Some(Self::new(
            unit_component(x, vector[0], square),
            unit_component(y, vector[1], square),
            unit_component(z, vector[2], square),
        ))
    }

    /// The unit vector from `self` towards `other`; `None` when they coincide.
    pub fn direction_to(self, other: Self) -> Option<Self> {
        other.checked_sub(self)?.normalized()
    }

    /// The point `step` along the way to `target`, or `target` when it is at most `step` away.
    /// Each component moves by `offset · step / distance` of the exact distance, rounded once to
    /// the nearest 2⁻²⁴, ties to even, as `normalized` rounds a unit, so it never passes the
    /// target's. `None` when the offset does not fit a `Num`, or has a component past ±2⁴⁷ bits,
    /// 2²³ m, beyond any offset of two places.
    pub fn checked_step_toward(self, target: Self, step: Num) -> Option<Self> {
        debug_assert!(step >= Num::ZERO, "Vec3::step_toward with a negative step");
        if self.within(target, step) {
            return Some(target);
        }
        let offset = target.checked_sub(self)?;
        let vector = offset.wide_axes()?;
        // The exact distance is above `step`, so each move is below its offset.
        let direction = Direction::of(vector).expect("an offset past the step is no zero vector");
        let square = offset.length_squared_bits();
        let step = step.to_bits().unsigned_abs();
        let unit = direction.unit();
        let moves: [Num; 3] =
            [0, 1, 2].map(|axis| step_component(unit[axis], vector[axis], step, square));
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
        // Each product fits an `i128`; their sum does not for every pair, as no `SinCos` but
        // its own constructors' keeps it a unit pair.
        let (Some(turned_x), Some(turned_z)) = (
            (x * cos).checked_add(z * sin),
            (z * cos).checked_sub(x * sin),
        ) else {
            return None;
        };
        match (
            Num::from_raw_products(turned_x),
            Num::from_raw_products(turned_z),
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

    /// Turned about the z axis by the angle of `turn`, from x towards y, so a positive angle
    /// raises x; each component rounded once from the exact sum of products.
    pub const fn checked_rotated_z(self, turn: SinCos) -> Option<Self> {
        let (x, y) = (self.x.to_bits() as i128, self.y.to_bits() as i128);
        let (sin, cos) = (turn.sin.to_bits() as i128, turn.cos.to_bits() as i128);
        let (Some(turned_x), Some(turned_y)) = (
            (x * cos).checked_sub(y * sin),
            (x * sin).checked_add(y * cos),
        ) else {
            return None;
        };
        match (
            Num::from_raw_products(turned_x),
            Num::from_raw_products(turned_y),
        ) {
            (Some(x), Some(y)) => Some(Self::new(x, y, self.z)),
            _ => None,
        }
    }

    #[must_use]
    pub const fn rotated_z(self, turn: SinCos) -> Self {
        match self.checked_rotated_z(turn) {
            Some(turned) => turned,
            None => panic!("Vec3 overflow in rotated_z"),
        }
    }

    /// A vector when all three components exist.
    const fn from_axes(x: Option<Num>, y: Option<Num>, z: Option<Num>) -> Option<Self> {
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

/// The component of the unit vector of a vector whose squared length is `square`, of the
/// vector's `component`, rounded once to the nearest 2⁻²⁴: from `unit`, the direction's axis at
/// 2⁻⁶², within 2⁻⁶² of the true one, the nearest of the two 2⁻²⁴ steps it lies between unless
/// it lies within 2 units of 2⁻⁶² of their half, where the exact half decides: the true
/// magnitude `m / √square` passes `n + ½` exactly when `4m² · 2⁴⁸ > (2n + 1)² · square`. It is
/// never a half: that takes `m · 2²⁵ / √square` odd, so the square's two-adic valuation 50 more
/// than twice the component's, where a sum of three squares, one of the least valuation odd,
/// has one of at most twice that least valuation and one.
fn unit_component(unit: i64, component: i128, square: u128) -> Num {
    const SHIFT: u32 = 62 - Num::FRAC_BITS;
    const HALF: u64 = 1 << (SHIFT - 1);
    let magnitude = unit.unsigned_abs();
    let floor = magnitude >> SHIFT;
    let rest = magnitude & ((1 << SHIFT) - 1);
    let up = if rest.abs_diff(HALF) > 2 {
        rest > HALF
    } else {
        let twice = component.unsigned_abs() << (Num::FRAC_BITS + 1);
        let odd = u128::from(2 * floor + 1);
        WideProduct::of(twice, twice) > WideProduct::of(odd * odd, square)
    };
    let bits = (floor + u64::from(up)).cast_signed();
    Num::from_bits(if unit < 0 { -bits } else { bits })
}

/// The move along an axis of a step of `step` bits toward an offset whose squared length is
/// `square`, of the offset's `component`: `component · step / √square`, rounded once to the
/// nearest bit, ties to even. From `unit`, the direction's axis at 2⁻⁶², within 2⁻⁶² of the true
/// one, the estimate lies within one bit of the move, which the exact comparison of
/// `(2 · component · step)²` with `(2m ± 1)² · square` then decides, as `unit_component` decides a
/// unit's half.
fn step_component(unit: i64, component: i128, step: u64, square: u128) -> Num {
    let estimate = (u128::from(unit.unsigned_abs()) * u128::from(step) + (1 << 61)) >> 62;
    let twice = 2 * component.unsigned_abs() * u128::from(step);
    let wanted = WideProduct::of(twice, twice);
    let odd = |m: u128| WideProduct::of((2 * m + 1) * (2 * m + 1), square);
    let mut moved = estimate;
    let above = odd(moved);
    if wanted > above || (wanted == above && moved % 2 == 1) {
        moved += 1;
    } else if moved > 0 {
        let below = odd(moved - 1);
        if wanted < below || (wanted == below && moved % 2 == 1) {
            moved -= 1;
        }
    }
    let bits = i64::try_from(moved).expect("a move is below its offset");
    Num::from_bits(if component < 0 { -bits } else { bits })
}

/// A product of two `u128`s, exact: its high and its low 128 bits, which order as the product
/// does, the high first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct WideProduct {
    high: u128,
    low: u128,
}

impl WideProduct {
    /// `a · b`.
    const fn of(a: u128, b: u128) -> Self {
        const LOW: u128 = (1 << 64) - 1;
        let (a_high, a_low) = (a >> 64, a & LOW);
        let (b_high, b_low) = (b >> 64, b & LOW);
        let low = a_low * b_low;
        let middle_a = a_high * b_low;
        let middle_b = a_low * b_high;
        let (middle, middle_carry) = middle_a.overflowing_add(middle_b);
        let (low, low_carry) = low.overflowing_add(middle << 64);
        let high =
            a_high * b_high + (middle >> 64) + ((middle_carry as u128) << 64) + low_carry as u128;
        Self { high, low }
    }
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
