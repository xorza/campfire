use bevy_ecs::resource::Resource;

use crate::values::grid::Grid;

/// The map's grid that sight reveals, and how many teams the match holds: package data, not
/// state. A restore takes it from the map, as a new match does.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VisionGrid {
    pub(crate) grid: Grid,
    pub(crate) teams: usize,
}
