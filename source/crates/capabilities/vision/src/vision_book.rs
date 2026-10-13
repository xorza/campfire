use bevy_ecs::resource::Resource;
use campfire_core::{Book, View};
use campfire_sim::Capability;

use crate::vision_grid::VisionGrid;

/// The map's vision grid; none in a map with no grid, where every unit sees all. Map data, not
/// state: a restore takes it from the map, as a new match does.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct VisionBook(pub Option<VisionGrid>);

impl VisionBook {
    pub const fn grid(&self) -> Option<&VisionGrid> {
        self.0.as_ref()
    }
}

impl Book for VisionBook {
    const OWNER: Option<Capability> = Some(Capability::Vision);
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &View) {}
}
