use bevy_ecs::resource::Resource;
use campfire_declarations::Book;
use campfire_script::MatchNames;

use crate::vision_grid::VisionGrid;

/// The map's vision grid; none in a map with no grid, where every unit sees all. Map data, not
/// state: a restore takes it from the map, as a new match does.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
#[component(immutable)]
pub struct VisionBook(pub Option<VisionGrid>);

impl VisionBook {
    pub const fn grid(&self) -> Option<&VisionGrid> {
        self.0.as_ref()
    }

    /// Whether the map has a grid, which what sees needs.
    pub(crate) const fn has_grid(&self) -> bool {
        self.0.is_some()
    }
}

impl Book for VisionBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &mut MatchNames) {}
}
