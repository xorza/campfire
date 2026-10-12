use bevy_ecs::resource::Resource;
use campfire_sim::Capability;

use crate::declare::book::Book;
use crate::navigation::paths::Paths;
use crate::navigation::walls::Walls;
use crate::scripts::frame::Frame;
use crate::units::view::View;

/// The map's paths, which walkers go along, and its walls, which block cells and placements. Map
/// data, not state: a restore takes it from the map, as a new match does.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct NavigationBook {
    pub(crate) paths: Paths,
    pub(crate) walls: Walls,
}

impl Book for NavigationBook {
    const OWNER: Option<Capability> = Some(Capability::Navigation);
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, view: &View, _: Option<&mut Frame>) {
        view.names_mut().set_paths(self.paths.shared_names());
    }
}
