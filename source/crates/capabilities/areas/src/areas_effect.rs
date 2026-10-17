use bevy_ecs::world::World;
use campfire_deliveries::Delivering;
use campfire_math::Vec3;
use campfire_script::{Effect, Frame};
use campfire_sim::StableId;
use campfire_units::MapBook;
use campfire_values::UnitType;

use crate::Areas;

/// An area a call queued: of `by`, of `unit_type`, at `at`; with `id`, the id the call took for
/// it, which a script reads, or none for a launch of a list, which takes its id as it lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AreasEffect {
    pub id: Option<StableId>,
    pub(crate) by: Delivering,
    pub(crate) unit_type: UnitType,
    pub pos: Vec3,
}

impl Effect for AreasEffect {
    /// Applies the effect: an area that lands this tick, at the point of the map's bounds nearest
    /// where it says.
    fn apply(self, world: &mut World, _: &mut Frame) {
        let pos = world.resource::<MapBook>().bounds.clamp(self.pos);
        Areas::push(world, self.by, self.unit_type, pos, None, self.id);
    }
}
