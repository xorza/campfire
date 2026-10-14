use std::cmp::Ordering;

use campfire_math::{Direction, Num, RoundedRoot, Vec3};

/// How near a straight path comes to a body: its nearest distance against a reach, and the place
/// along the path of its nearest point, both to the nearest bit (design 09, D9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Approach {
    pub nearest: Ordering,
    pub(crate) place: Num,
}

impl Approach {
    /// The approach of the path `path` to a circle of `radius` centred `centre` from its start,
    /// against `reach`: to the centre against the sum, which past every number reaches every
    /// distance.
    pub(crate) fn of_circle(path: Vec3, centre: Vec3, reach: Num, radius: Num) -> Approach {
        Approach::of(path, centre, reach.checked_add(radius).unwrap_or(Num::MAX))
    }

    /// The approach of the path `path`, the offset from its start to its end, to the point `off`
    /// from its start, against `reach`; a negative reach is nearer than any distance. Beyond an
    /// end the distance is the end's, its nearest root measured against the reach with no root;
    /// between them it is the distance from the path's line.
    pub fn of(path: Vec3, off: Vec3, reach: Num) -> Approach {
        if reach < Num::ZERO {
            return Approach {
                nearest: Ordering::Greater,
                place: Num::ZERO,
            };
        }
        let reach = u128::from(reach.to_bits().cast_unsigned());
        let (path_bits, off_bits) = (path.to_bits(), off.to_bits());
        let along: i128 = off_bits.iter().zip(path_bits).map(|(a, b)| a * b).sum();
        let at_start = Approach {
            nearest: off.length_squared_bits().nearest_root_cmp(reach),
            place: Num::ZERO,
        };
        let Some(direction) = Direction::of(path_bits) else {
            return at_start;
        };
        if along <= 0 {
            return at_start;
        }
        let place = |bits: i128| Num::from_bits(i64::try_from(bits).expect("a place on a path"));
        if along.cast_unsigned() >= path.length_squared_bits() {
            return Approach {
                nearest: (off - path).length_squared_bits().nearest_root_cmp(reach),
                place: place(direction.length()),
            };
        }
        Approach {
            nearest: direction.apart(off_bits).cast_unsigned().cmp(&reach),
            place: place(direction.along(off_bits)),
        }
    }
}
