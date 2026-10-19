use campfire_math::Vec3;

/// A straight segment between two points on the ground plane; from a point to itself, that
/// point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    from: Vec3,
    to: Vec3,
}

impl Segment {
    pub const fn new(from: Vec3, to: Vec3) -> Self {
        Self { from, to }
    }

    pub(crate) const fn start(self) -> Vec3 {
        self.from
    }

    pub(crate) const fn end(self) -> Vec3 {
        self.to
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_geometry::internals::path_comes_closer;
    use campfire_math::{Num, Vec3};

    use crate::segment::Segment;

    impl Segment {
        /// Whether the segment comes closer than `reach` to `position`, exactly; touching at
        /// `reach` is not closer: the circles' test, which the shapes' must agree with.
        pub fn comes_closer(self, position: Vec3, reach: Num) -> bool {
            let path = self.from.ground_offset(self.to);
            let off = self.from.ground_offset(position);
            path_comes_closer(path, off, reach)
        }
    }
}

#[cfg(test)]
mod tests;
