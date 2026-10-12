use bevy_ecs::resource::Resource;
use campfire_sim::Capability;

use crate::declare::book::Book;
use crate::scripts::frame::Frame;
use crate::units::view::View;
use crate::vision::vision_grid::VisionGrid;

/// The map's vision grid; none in a map with no grid, where every unit sees all. Map data, not
/// state: a restore takes it from the map, as a new match does.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct VisionBook(pub(crate) Option<VisionGrid>);

impl VisionBook {
    pub(crate) const fn grid(&self) -> Option<&VisionGrid> {
        self.0.as_ref()
    }
}

impl Book for VisionBook {
    const OWNER: Option<Capability> = Some(Capability::Vision);
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &View, _: Option<&mut Frame>) {}
}
