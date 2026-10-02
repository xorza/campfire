use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_math::Vec3;
use campfire_sim::{Position, StableId};

use crate::deliveries::delivering::Delivering;
use crate::projectiles::Projectiles;
use crate::scripts::effects::Effect;
use crate::scripts::frame::Frame;
use crate::units::unit_type::UnitType;

/// A projectile a call queued: of `by`, of `unit_type`, from `from`, toward a direction or homing
/// on a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProjectileEffect {
    pub(crate) by: Delivering,
    pub(crate) unit_type: UnitType,
    pub(crate) from: Position,
    pub(crate) toward: Toward,
}

/// Where a projectile flies: along a direction, or homing on a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Toward {
    Direction(Vec3),
    Unit(StableId),
}

impl Effect for ProjectileEffect {
    fn apply(self, world: &mut World, _: &mut Frame, _: Tick) {
        Projectiles::apply(world, self);
    }
}
