use std::cmp::Ordering;

use campfire_math::{Direction, FloorRoot, Num, RoundedRoot, Vec3};

/// How near a straight path comes to a body: its nearest distance against a reach, and the place
/// along the path of its nearest point, both to the nearest bit (design 09, D9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Approach {
    pub nearest: Ordering,
    pub(crate) place: Num,
}

/// Where a path comes nearest a point: at its start, at its end, or between them, where the
/// distance is the path's line's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stretch {
    Start,
    End,
    Between,
}

impl Approach {
    /// The approach of the path `path` to a circle of `radius` centered `center` from its start,
    /// against `reach`: to the center against the sum, which past every number reaches every
    /// distance.
    pub(crate) fn of_circle(path: Vec3, center: Vec3, reach: Num, radius: Num) -> Approach {
        Approach::of(path, center, Approach::circle_reach(reach, radius))
    }

    /// `of_circle`'s order alone, which takes no direction where the path comes nearest at an
    /// end.
    pub(crate) fn nearest_of_circle(path: Vec3, center: Vec3, reach: Num, radius: Num) -> Ordering {
        Approach::nearest_of(path, center, Approach::circle_reach(reach, radius))
    }

    /// The approach of the path `path`, the offset from its start to its end, to the point `off`
    /// from its start, against `reach`; a negative reach is nearer than any distance. Beyond an
    /// end the distance is the end's, its nearest root measured against the reach with no root;
    /// between them it is the distance from the path's line.
    pub fn of(path: Vec3, off: Vec3, reach: Num) -> Approach {
        let Some(reach) = Approach::units(reach) else {
            return Approach {
                nearest: Ordering::Greater,
                place: Num::ZERO,
            };
        };
        let place = |bits: i128| Num::from_bits(i64::try_from(bits).expect("a place on a path"));
        match Approach::stretch(path, off) {
            Stretch::Start => Approach {
                nearest: off.length_squared_bits().nearest_root_cmp(reach),
                place: Num::ZERO,
            },
            Stretch::End => Approach {
                nearest: (off - path).length_squared_bits().nearest_root_cmp(reach),
                place: place(Approach::direction(path).length()),
            },
            Stretch::Between => {
                let direction = Approach::direction(path);
                Approach {
                    nearest: direction.apart(off.to_bits()).cast_unsigned().cmp(&reach),
                    place: place(direction.along(off.to_bits())),
                }
            }
        }
    }

    /// `of`'s order alone, which takes no direction where the path comes nearest at an end.
    pub fn nearest_of(path: Vec3, off: Vec3, reach: Num) -> Ordering {
        let Some(reach) = Approach::units(reach) else {
            return Ordering::Greater;
        };
        match Approach::stretch(path, off) {
            Stretch::Start => off.length_squared_bits().nearest_root_cmp(reach),
            Stretch::End => (off - path).length_squared_bits().nearest_root_cmp(reach),
            Stretch::Between => Approach::between(path, off, reach),
        }
    }

    /// How the distance of `off` from the line of `path`, as the path's direction measures it,
    /// to the nearest unit and within 1.4 of the true one (design 09, D9), lies against `reach`.
    /// The true distance is `|C| / |path|`, with `C` the exact cross product of `off` and `path`,
    /// and `|C|` lies from its largest axis to the sum of its axes, one and the same on a planar
    /// map: the largest axis at least `(reach + 3) ⌈|path|⌉` puts the distance at least three
    /// units past the reach, so its measure past it; the sum below `(reach − 2) ⌊|path|⌋` more
    /// than two units short, so its measure short of it. Only the band between takes the
    /// direction. Offsets within twice the world's bound keep each product below 2⁹², and each
    /// bound below 2¹¹¹, so the wrapping steps skip the checks a release build makes.
    fn between(path: Vec3, off: Vec3, reach: u128) -> Ordering {
        let ([px, py, pz], [ox, oy, oz]) = (path.to_bits(), off.to_bits());
        let cross = |a: i128, b: i128, c: i128, d: i128| {
            a.wrapping_mul(b)
                .wrapping_sub(c.wrapping_mul(d))
                .unsigned_abs()
        };
        let axes = [
            cross(oy, pz, oz, py),
            cross(oz, px, ox, pz),
            cross(ox, py, oy, px),
        ];
        let largest = axes[0].max(axes[1]).max(axes[2]);
        let sum = axes[0].wrapping_add(axes[1]).wrapping_add(axes[2]);
        let square = path.length_squared_bits();
        let root = square.floor_root();
        let ceiling = root.wrapping_add(u128::from(root.wrapping_mul(root) < square));
        if largest >= reach.wrapping_add(3).wrapping_mul(ceiling) {
            Ordering::Greater
        } else if reach >= 2 && sum < (reach - 2).wrapping_mul(root) {
            Ordering::Less
        } else {
            let apart = Approach::direction(path).apart(off.to_bits());
            apart.cast_unsigned().cmp(&reach)
        }
    }

    /// Where `path` comes nearest `off`: at its start when `off` lies at or behind it, a path of
    /// no length among them; at its end at or past it; else between.
    fn stretch(path: Vec3, off: Vec3) -> Stretch {
        let along: i128 = off
            .to_bits()
            .iter()
            .zip(path.to_bits())
            .map(|(a, b)| a * b)
            .sum();
        if along <= 0 {
            Stretch::Start
        } else if along.cast_unsigned() >= path.length_squared_bits() {
            Stretch::End
        } else {
            Stretch::Between
        }
    }

    /// `reach` in bits, as a distance's nearest root orders against it; `None` when negative.
    fn units(reach: Num) -> Option<u128> {
        (reach >= Num::ZERO).then(|| u128::from(reach.to_bits().cast_unsigned()))
    }

    /// The direction of a path that runs past its start.
    fn direction(path: Vec3) -> Direction<3> {
        Direction::of(path.to_bits()).expect("a path that runs past its start has a length")
    }

    /// A reach and a radius, summed, which past every number reaches every distance.
    fn circle_reach(reach: Num, radius: Num) -> Num {
        reach.checked_add(radius).unwrap_or(Num::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_order_near_the_reach_between_the_ends_is_the_directions() {
        // Points within 2 bits on each axis of the point the reach straight out from each path's
        // middle, at reaches either side of its distance: where the bounds leave the order to the
        // path's direction, and either side of them, against the full approach's.
        let paths: [[i128; 3]; 3] = [
            [1 << 20, 0, 0],
            [5 << 18, 0, -(3 << 18)],
            [700_001, 300_007, -123_457],
        ];
        for path in paths {
            let [px, py, pz] = path;
            // Perpendicular to the path: its cross product with the vertical, or with x.
            let normal = if px == 0 && pz == 0 {
                [0, pz, -py]
            } else {
                [-pz, 0, px]
            };
            let length = normal
                .iter()
                .map(|&axis| (axis * axis).cast_unsigned())
                .sum::<u128>()
                .floor_root()
                .cast_signed();
            for distance in [3, 1000, 1 << 20] {
                let target =
                    [0, 1, 2].map(|axis| path[axis] / 2 + normal[axis] * distance / length);
                for corner in 0..125 {
                    let shift = [corner % 5, corner / 5 % 5, corner / 25].map(|step| step - 2);
                    let off = [0, 1, 2].map(|axis| target[axis] + shift[axis]);
                    let vector = |bits: [i128; 3]| {
                        let num = |axis: i128| Num::from_bits(i64::try_from(axis).unwrap());
                        Vec3::new(num(bits[0]), num(bits[1]), num(bits[2]))
                    };
                    let (path, off) = (vector(path), vector(off));
                    for reach in [
                        distance - 3,
                        distance - 1,
                        distance,
                        distance + 1,
                        distance + 3,
                    ] {
                        let reach = Num::from_bits(i64::try_from(reach).unwrap());
                        assert_eq!(
                            Approach::nearest_of(path, off, reach),
                            Approach::of(path, off, reach).nearest,
                            "{path:?} {off:?} {reach:?}"
                        );
                    }
                }
            }
        }
    }
}
