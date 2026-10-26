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

    /// The most cells times teams a match with vision holds: each team's group holds a count of 4
    /// bytes on each cell of its sight and of its detection, so a map's grid takes at most
    /// 2²³ · 2 · 4 bytes, 64 MiB, of counts, as the largest grid, 2²² cells, does for two teams.
    pub const MAX_CELL_TEAMS: usize = 1 << 23;
}
