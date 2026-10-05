use campfire_math::Num;
use campfire_sim::Position;

/// A box of the map's ground plane, from `min` to `max`, edges included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Region {
    min: [Num; 2],
    max: [Num; 2],
}

impl Region {
    /// The box from `min` to `max`, `[x, z]` each, `min` below `max` on both axes.
    pub(crate) fn new(min: [Num; 2], max: [Num; 2]) -> Region {
        debug_assert!(
            min[0] < max[0] && min[1] < max[1],
            "a region's min is below its max"
        );
        Region { min, max }
    }

    /// Whether `pos` lies within it on the ground plane, its edges included.
    pub(crate) fn contains(self, pos: Position) -> bool {
        let at = pos.get();
        (self.min[0]..=self.max[0]).contains(&at.x) && (self.min[1]..=self.max[1]).contains(&at.z)
    }
}
