use campfire_geometry::Grid;

use crate::brush_map::BrushMap;

/// The map's grid that sight reveals, its brush, and how many teams the match holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisionGrid {
    pub grid: Grid,
    pub brush: BrushMap,
    pub teams: usize,
}

impl VisionGrid {
    /// The most teams a match with vision holds, so its groups' bitmaps stay within what a map's
    /// cells times this many costs.
    pub const MAX_TEAMS: usize = 64;
}
