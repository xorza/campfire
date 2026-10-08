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
}

#[cfg(test)]
pub(crate) mod internals {
    use std::cmp::Ordering;

    use campfire_math::Num;
    use campfire_sim::Position;

    use crate::geometry::approach::Approach;
    use crate::geometry::metric::Metric;
    use crate::navigation::segment::Segment;

    impl Segment {
        /// Whether the segment comes closer than `reach` to `at`, exactly; touching at `reach`
        /// is not closer: the circles' test, which the shapes' must agree with.
        pub(crate) fn comes_within(self, at: Position, reach: Num) -> bool {
            let path = Metric::Planar.offset(self.from, self.to);
            let off = Metric::Planar.offset(self.from, at);
            Approach::of(path, off, reach).nearest == Ordering::Less
        }
    }
}

#[cfg(test)]
mod tests;
