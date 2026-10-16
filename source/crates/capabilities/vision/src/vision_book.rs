use bevy_ecs::resource::Resource;
use bevy_ecs::system::Res;
use campfire_declare::Book;
use campfire_script::View;

use crate::vision_grid::VisionGrid;

/// The map's vision grid; none in a map with no grid, where every unit sees all. Map data, not
/// state: a restore takes it from the map, as a new match does.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct VisionBook(pub Option<VisionGrid>);

impl VisionBook {
    pub const fn grid(&self) -> Option<&VisionGrid> {
        self.0.as_ref()
    }

    /// The run condition of what sees: whether the map has a grid.
    pub(crate) fn has_grid(book: Res<'_, Self>) -> bool {
        book.0.is_some()
    }
}

impl Book for VisionBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &View) {}
}
