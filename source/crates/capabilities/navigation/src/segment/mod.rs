use campfire_sim::Position;

/// A straight segment between two points on the ground plane; from a point to itself, that
/// point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    from: Position,
    to: Position,
}

impl Segment {
    pub const fn new(from: Position, to: Position) -> Segment {
        Segment { from, to }
    }

    pub(crate) const fn start(self) -> Position {
        self.from
    }

    pub(crate) const fn end(self) -> Position {
        self.to
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::cmp::Ordering;

    use campfire_geometry::{Approach, Metric};
    use campfire_math::Num;
    use campfire_sim::Position;

    use crate::segment::Segment;

    impl Segment {
        /// Whether the segment comes closer than `reach` to `pos`, exactly; touching at `reach`
        /// is not closer: the circles' test, which the shapes' must agree with.
        pub fn comes_within(self, pos: Position, reach: Num) -> bool {
            let path = Metric::Planar.offset(self.from, self.to);
            let off = Metric::Planar.offset(self.from, pos);
            Approach::nearest_of(path, off, reach) == Ordering::Less
        }
    }
}

#[cfg(test)]
mod tests;
