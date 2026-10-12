use bevy_ecs::resource::Resource;

use crate::geometry::grid::Grid;
use crate::vision::brush_map::BrushMap;

/// The map's grid that sight reveals, its brush, and how many teams the match holds: package
/// data, not state. A restore takes it from the map, as a new match does.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub(crate) struct VisionGrid {
    pub(crate) grid: Grid,
    pub(crate) brush: BrushMap,
    pub(crate) teams: usize,
}

impl VisionGrid {
    /// The most teams a match with vision holds, so its groups' bitmaps stay within what a map's
    /// cells times this many costs.
    pub(crate) const MAX_TEAMS: usize = 64;
}
