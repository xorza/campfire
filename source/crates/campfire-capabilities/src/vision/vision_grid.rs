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

#[cfg(test)]
pub(crate) mod internals {
    use bevy_ecs::world::World;
    use campfire_math::Num;

    use crate::geometry::bounds::Bounds;
    use crate::geometry::grid::Grid;
    use crate::vision::brush_map::BrushMap;
    use crate::vision::vision_grid::VisionGrid;

    impl VisionGrid {
        /// Gives `world` a grid of one cell, for `teams` teams, that no Vision stage reads: the
        /// `SeenBy` a test sets on its units alone tells what each team sees.
        pub(crate) fn fog(world: &mut World, teams: usize) {
            let bounds = Bounds::new([-Num::HALF; 2], [Num::HALF; 2]).unwrap();
            let grid = Grid::new(Num::ONE, bounds).unwrap();
            let brush = BrushMap::new(&grid, &[]);
            world.insert_resource(VisionGrid { grid, brush, teams });
        }
    }
}
