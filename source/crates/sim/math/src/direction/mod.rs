use crate::root::RoundedRoot;
use crate::rounding::Rounding;

#[cfg(feature = "internals")]
pub(crate) mod bench;

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
        let square: u128 = vector
            .iter()
            .map(|&axis| (axis * axis).cast_unsigned())
            .sum();
        if square == 0 {
            return None;
        }
        let fine = (126 - (128 - square.leading_zeros())) / 2;
        let root = (square << (2 * fine)).nearest_root().cast_signed();
        // An axis is at most the length, below 2ʷ for a square of 2w bits, so shifted it stays
        // below 2^(w + 62 + fine) ≤ 2¹²⁵.
        let unit = vector.map(|axis| {
            let axis = Rounding::NearestEven.divide(axis << (FRACTION + fine), root);
            i64::try_from(axis).expect("a unit's axis is at most 1")
        });
        let length = if fine == 0 {
            root
        } else {
            Rounding::NearestEven.shift_right(root, fine)
        };
        Some(Direction { unit, length })
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
            .map(|(&axis, unit)| axis * i128::from(unit))
            .sum();
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
        self.unit.map(|unit| {
            let magnitude = i128::from(unit.unsigned_abs()) * length + cover;
            Rounding::Ceiling.shift_right(magnitude, FRACTION) * i128::from(unit.signum())
        })
    }
}

impl Direction<2> {
    /// How far `off`, each axis within ±2⁴⁷, lies from the line along the direction through the
    /// origin: its cross product with the unit vector, rounded to the nearest unit.
    pub fn apart(&self, off: [i128; 2]) -> i128 {
        debug_assert!(off.iter().all(|axis| axis.abs() < WIDEST));
        let [x, z] = off;
        let [unit_x, unit_z] = self.unit.map(i128::from);
        Rounding::NearestEven
            .shift_right(x * unit_z - z * unit_x, FRACTION)
            .abs()
    }
}

impl Direction<3> {
    /// How far `off`, each axis within ±2⁴⁷, lies from the line along the direction through the
    /// origin: the length of its cross product with the unit vector, each axis rounded to the
    /// nearest unit, then the nearest root of their squares, below 2⁹⁸.
    pub fn apart(&self, off: [i128; 3]) -> i128 {
        debug_assert!(off.iter().all(|axis| axis.abs() < WIDEST));
        let [x, y, z] = off;
        let [unit_x, unit_y, unit_z] = self.unit.map(i128::from);
        let cross = [
            y * unit_z - z * unit_y,
            z * unit_x - x * unit_z,
            x * unit_y - y * unit_x,
        ]
        .map(|axis| Rounding::NearestEven.shift_right(axis, FRACTION));
        let square: u128 = cross
            .iter()
            .map(|&axis| (axis * axis).cast_unsigned())
            .sum();
        square.nearest_root().cast_signed()
    }
}

#[cfg(test)]
mod tests;
