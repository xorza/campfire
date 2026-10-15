use std::ops::Neg;

use crate::direction::unit_divisor::UnitDivisor;
use crate::root;
use crate::root::{RoundedRoot, WideRoot};
use crate::rounding::Rounding;

#[cfg(feature = "internals")]
pub(crate) mod bench;
mod unit_divisor;

/// The bits of fraction of each axis of a unit vector.
const FRACTION: u32 = 62;
/// The widest axis a direction is taken from, or an offset it measures, exclusive: positions lie
/// within ±2⁴⁴ bits, so offsets, and a box's at twice the scale, within ±2⁴⁷. Each product with a
/// unit axis stays below 2¹⁰⁹, and a sum of three below 2¹¹¹.
const WIDEST: i128 = 1 << 47;

/// The direction of a vector of `N` axes, in any integer unit, as bits or halves of a bit: its unit
/// vector, each axis with 62 bits of fraction, and its length, each rounded to nearest. Each
/// measure it takes of an offset, how far along it and how far from its line, rounds once more,
/// to the nearest unit: along, and apart in two axes, within 0.51 of a unit of the true value;
/// apart in three axes, within 1.4 (design 09, D9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Direction<const N: usize> {
    unit: [i64; N],
    length: i128,
}

impl<const N: usize> Direction<N> {
    /// The direction of `vector`, each axis within ±2⁴⁷; `None` for the vector of no length.
    /// Its length is a root with as many bits of fraction as 126 bits leave beside its square, at
    /// least 62, so the unit's axes lie within 2⁻⁶² of the true ones.
    pub fn of(vector: [i128; N]) -> Option<Direction<N>> {
        debug_assert!(vector.iter().all(|axis| axis.abs() < WIDEST));
        let square = Direction::square(vector);
        if square == 0 {
            return None;
        }
        let fine = (126 - (128 - square.leading_zeros())) / 2;
        let WideRoot {
            root,
            half_reciprocal,
        } = root::nearest_wide(square << (2 * fine));
        let divisor = UnitDivisor::new(root, half_reciprocal, FRACTION + fine);
        let unit = vector.map(|axis| divisor.unit(axis));
        Some(Direction {
            unit,
            length: Direction::<N>::unscaled(root, fine),
        })
    }

    /// The square of the length of `vector`, each axis within ±2⁴⁷: each square, one widening
    /// product, is below 2⁹⁴, so three cannot wrap, and the wrapping steps skip the checks a
    /// release build makes.
    #[expect(clippy::cast_possible_truncation, reason = "an axis below 2⁴⁷")]
    fn square(vector: [i128; N]) -> u128 {
        vector.iter().fold(0, |sum: u128, &axis| {
            let magnitude = u128::from(axis.unsigned_abs() as u64);
            sum.wrapping_add(magnitude.wrapping_mul(magnitude))
        })
    }

    /// `root / 2^fine`, rounded to nearest, ties to even.
    const fn unscaled(root: u64, fine: u32) -> i128 {
        debug_assert!(root <= 1 << 63 && fine <= 62);
        if fine == 0 {
            return root as i128;
        }
        Rounding::NearestEven.shift_right(root as i128, fine)
    }

    /// The length of the vector it was taken from, rounded to the nearest unit.
    pub const fn length(&self) -> i128 {
        self.length
    }

    /// How far `off`, each axis within ±2⁴⁷, lies along the direction, negative behind: its dot
    /// product with the unit vector, rounded to the nearest unit.
    pub fn along(&self, off: [i128; N]) -> i128 {
        debug_assert!(off.iter().all(|axis| axis.abs() < WIDEST));
        let dot = off
            .iter()
            .zip(self.unit)
            .fold(0, |dot: i128, (&axis, unit)| {
                dot.wrapping_add(Direction::<N>::product(axis, unit))
            });
        Rounding::NearestEven.shift_right(dot, FRACTION)
    }

    /// The vector along the direction `length` units long, `length` from 0 to 2⁴⁷, each axis's
    /// magnitude rounded up: at least the true axis's, as the length's one part in 2⁶² added
    /// before the rounding covers the unit's own, and less than a unit and that part past it. A
    /// direction along an axis has an exact unit, which takes no such part. What a push moves by,
    /// so a body ends at least `length` away.
    pub fn scaled_away(&self, length: i128) -> [i128; N] {
        debug_assert!((0..WIDEST).contains(&length));
        let exact = self
            .unit
            .iter()
            .all(|&unit| unit == 0 || unit.unsigned_abs() == 1 << FRACTION);
        let cover = if exact { 0 } else { length };
        // Each magnitude is below 2¹⁰⁹, so adding a unit less one rounds up with no wrap.
        self.unit.map(|unit| {
            let magnitude = (Direction::<N>::product(length, unit).unsigned_abs()
                + cover.cast_unsigned())
            .wrapping_add((1 << FRACTION) - 1)
                >> FRACTION;
            let magnitude = magnitude.cast_signed();
            match unit.signum() {
                1 => magnitude,
                -1 => -magnitude,
                _ => 0,
            }
        })
    }

    /// `axis · unit`, exact: an axis within ±2⁴⁷ and a unit's axis within ±2⁶², one widening
    /// product of two `i64`s, below 2¹⁰⁹, which cannot overflow.
    #[expect(clippy::cast_possible_truncation, reason = "an axis within ±2⁴⁷")]
    const fn product(axis: i128, unit: i64) -> i128 {
        (axis as i64 as i128).wrapping_mul(unit as i128)
    }
}

impl Direction<2> {
    /// The direction a quarter turn clockwise, of `[z, −x]` for a direction of `[x, z]`: what `of`
    /// gives for that vector, as its square, its root and its length are the same, and each axis
    /// of the unit rounds as its negation does.
    #[must_use]
    pub const fn turned(&self) -> Direction<2> {
        Direction {
            unit: [self.unit[1], self.unit[0].wrapping_neg()],
            length: self.length,
        }
    }

    /// How far `off`, each axis within ±2⁴⁷, lies from the line along the direction through the
    /// origin: its cross product with the unit vector, rounded to the nearest unit.
    pub fn apart(&self, off: [i128; 2]) -> i128 {
        debug_assert!(off.iter().all(|axis| axis.abs() < WIDEST));
        let [x, z] = off;
        let [unit_x, unit_z] = self.unit;
        let cross =
            Direction::<2>::product(x, unit_z).wrapping_sub(Direction::<2>::product(z, unit_x));
        Rounding::NearestEven.shift_right(cross, FRACTION).abs()
    }
}

impl Direction<3> {
    /// How far `off`, each axis within ±2⁴⁷, lies from the line along the direction through the
    /// origin: the length of its cross product with the unit vector, each axis rounded to the
    /// nearest unit, then the nearest root of their squares, below 2⁹⁸.
    #[expect(clippy::cast_possible_truncation, reason = "a cross axis below 2⁴⁹")]
    pub fn apart(&self, off: [i128; 3]) -> i128 {
        debug_assert!(off.iter().all(|axis| axis.abs() < WIDEST));
        let [x, y, z] = off;
        let [unit_x, unit_y, unit_z] = self.unit;
        let cross = |a: i128, unit_b: i64, b: i128, unit_a: i64| {
            let product = Direction::<3>::product;
            Rounding::NearestEven.shift_right(
                product(a, unit_b).wrapping_sub(product(b, unit_a)),
                FRACTION,
            )
        };
        // Each axis of the cross product is below 2⁴⁹ in magnitude, so a square is one widening
        // product below 2⁹⁸, and three cannot wrap.
        let square = [
            cross(y, unit_z, z, unit_y),
            cross(z, unit_x, x, unit_z),
            cross(x, unit_y, y, unit_x),
        ]
        .iter()
        .fold(0, |sum: u128, &axis| {
            let magnitude = u128::from(axis.unsigned_abs() as u64);
            sum.wrapping_add(magnitude.wrapping_mul(magnitude))
        });
        square.nearest_root().cast_signed()
    }
}

/// The opposite direction: what `of` gives for the negated vector, as each axis of the unit
/// rounds as its negation does. A unit's axis is at most 2⁶², so its negation cannot wrap.
impl<const N: usize> Neg for Direction<N> {
    type Output = Direction<N>;

    fn neg(self) -> Direction<N> {
        Direction {
            unit: self.unit.map(i64::wrapping_neg),
            length: self.length,
        }
    }
}

#[cfg(test)]
mod tests;
