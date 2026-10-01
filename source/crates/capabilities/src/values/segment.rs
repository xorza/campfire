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
