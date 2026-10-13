use bevy_ecs::resource::Resource;
use campfire_core::{Book, View};
use campfire_sim::Capability;

use crate::paths::Paths;
use crate::walls::Walls;

/// The map's paths, which walkers go along, and its walls, which block cells and placements. Map
/// data, not state: a restore takes it from the map, as a new match does.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct NavigationBook {
    pub paths: Paths,
    pub walls: Walls,
}

impl Book for NavigationBook {
    const OWNER: Option<Capability> = Some(Capability::Navigation);
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, view: &View) {
        view.names_mut().set_paths(self.paths.shared_names());
    }
}
