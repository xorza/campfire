use campfire_math::{Num, U256};
use campfire_sim::Position;

/// A straight segment between two points on the ground plane; from a point to itself, that
/// point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Segment {
    from: Position,
    to: Position,
}

impl Segment {
    pub(crate) const fn new(from: Position, to: Position) -> Segment {
        Segment { from, to }
    }

    pub(crate) const fn start(self) -> Position {
        self.from
    }

    pub(crate) const fn end(self) -> Position {
        self.to
    }

    /// Whether the segment comes closer than `reach` to `at`, exactly; touching at `reach` is not
    /// closer.
    pub(crate) fn comes_within(self, at: Position, reach: Num) -> bool {
        let ground = |pos: Position| [pos.get().x, pos.get().z].map(|v| i128::from(v.to_bits()));
        let [a, b, at] = [ground(self.from), ground(self.to), ground(at)];
        let reach = u128::from(reach.to_bits().unsigned_abs());
        let square = |v: [i128; 2]| (v[0] * v[0] + v[1] * v[1]).cast_unsigned();
        let within = |square: u128| square < reach * reach;
        let along = [b[0] - a[0], b[1] - a[1]];
        let from_a = [at[0] - a[0], at[1] - a[1]];
        let length = square(along);
        let projection = from_a[0] * along[0] + from_a[1] * along[1];
        if projection <= 0 {
            return within(square(from_a));
        }
        if projection.cast_unsigned() >= length {
            return within(square([at[0] - b[0], at[1] - b[1]]));
        }
        // The nearest point lies within the segment: its squared distance is
        // |from_a|² − projection² / length, compared times `length`.
        let far = U256::product(square(from_a), length);
        let near = U256::product(reach * reach, length)
            .checked_add(U256::product(
                projection.cast_unsigned(),
                projection.cast_unsigned(),
            ))
            .expect("points within the world's bound");
        far < near
    }
}

#[cfg(test)]
mod tests {
    use campfire_math::Vec3;

    use super::*;

    fn num(value: i64) -> Num {
        Num::from_int(value).unwrap()
    }

    fn at(x: i64, z: i64) -> Position {
        Position::new(Vec3::new(num(x), Num::ZERO, num(z))).unwrap()
    }

    #[test]
    fn a_segment_comes_within_a_reach_of_a_point_exactly() {
        let e = Num::EPSILON;
        // Along x from 0 to 10: (5, 1) is 1 m off it; (−3, 4) and (13, 4) are 5 m from an end,
        // before its start and past its end.
        let straight = Segment::new(at(0, 0), at(10, 0));
        for (point, distance) in [(at(5, 1), 1), (at(-3, 4), 5), (at(13, 4), 5)] {
            assert!(
                !straight.comes_within(point, num(distance)),
                "{point:?} touches"
            );
            assert!(
                straight.comes_within(point, num(distance) + e),
                "{point:?} within"
            );
        }
        // From (0, 0) to (8, 6), 10 m: (1, 7) projects halfway, onto (4, 3), √(3² + 4²) = 5 m
        // away.
        let slant = Segment::new(at(0, 0), at(8, 6));
        assert!(!slant.comes_within(at(1, 7), num(5)));
        assert!(slant.comes_within(at(1, 7), num(5) + e));
        // A segment from a point to itself is that point: (5, 6) is 5 m from (2, 2).
        let only = Segment::new(at(2, 2), at(2, 2));
        assert!(!only.comes_within(at(5, 6), num(5)));
        assert!(only.comes_within(at(5, 6), num(5) + e));
        // Across the whole world, 2²⁰ m, whose products pass 128 bits: (0, 1) is 1 m off.
        let half = 1 << 19;
        let wide = Segment::new(at(-half, 0), at(half, 0));
        assert!(!wide.comes_within(at(0, 1), num(1)));
        assert!(wide.comes_within(at(0, 1), num(1) + e));
    }
}
