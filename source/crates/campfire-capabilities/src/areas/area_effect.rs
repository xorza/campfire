use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_sim::Position;

use crate::areas::Areas;
use crate::deliveries::delivering::Delivering;
use crate::scripts::effects::Effect;
use crate::scripts::frame::Frame;
use crate::units::unit_type::UnitType;

/// An area a call queued: of `by`, of `unit_type`, at `at`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AreaEffect {
    pub(crate) by: Delivering,
    pub(crate) unit_type: UnitType,
    pub(crate) at: Position,
}

impl Effect for AreaEffect {
    fn apply(self, world: &mut World, _: &mut Frame, _: Tick) {
        Areas::apply(world, self);
    }
}
