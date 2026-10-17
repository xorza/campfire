use bevy_ecs::resource::Resource;
use campfire_declare::Book;
use campfire_geometry::Grid;
use campfire_script::MatchNames;

use crate::paths::Paths;
use crate::walls::Walls;

/// The map's paths, which walkers go along, and its walls, which block cells and placements. Map
/// data, not state: a restore takes it from the map, as a new match does.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct NavigationBook {
    pub paths: Paths,
    pub walls: Walls,
    /// The cells units plan routes over, when the map has them.
    pub pathing: Option<Grid>,
}

impl Book for NavigationBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, names: &mut MatchNames) {
        names.set_table(self.paths.shared_names());
    }
}
