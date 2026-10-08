use std::cmp::Ordering;

use campfire_math::{Num, U256, Vec3};

use crate::geometry::fraction::Fraction;

/// How near a straight path comes to a point: its nearest distance against a reach, and the share
/// of the path at its nearest point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Approach {
    pub(crate) nearest: Ordering,
    pub(crate) share: Fraction,
}

impl Approach {
    /// The approach of the path `path`, the offset from its start to its end, to the point `off`
    /// from its start, against `reach`, exactly and without a square root; a negative reach is
    /// nearer than any distance.
    pub(crate) fn of(path: Vec3, off: Vec3, reach: Num) -> Approach {
        let length = path.length_squared_bits();
        let raw = |v: Vec3| [v.x, v.y, v.z].map(|n| i128::from(n.to_bits()));
        let along: i128 = raw(off).iter().zip(raw(path)).map(|(a, b)| a * b).sum();
        if reach < Num::ZERO {
            return Approach {
                nearest: Ordering::Greater,
                share: Fraction::ZERO,
            };
        }
        let reach = u128::from(reach.to_bits().cast_unsigned());
        let reach = reach * reach;
        if length == 0 || along <= 0 {
            return Approach {
                nearest: off.length_squared_bits().cmp(&reach),
                share: Fraction::ZERO,
            };
        }
        let along = along.cast_unsigned();
        if along >= length {
            return Approach {
                nearest: (off - path).length_squared_bits().cmp(&reach),
                share: Fraction::ONE,
            };
        }
        // The squared distance from the line, times the squared length: exact in 256 bits.
        let apart = U256::product(off.length_squared_bits(), length);
        let allowed = U256::product(reach, length).checked_add(U256::product(along, along));
        let allowed = allowed.expect("squares of offsets within the world's bound fit 256 bits");
        // A squared length within the world's bound is below 2⁹⁴, so both fit an i128.
        Approach {
            nearest: apart.cmp(&allowed),
            share: Fraction::new(along.cast_signed(), length.cast_signed()),
        }
    }
}
