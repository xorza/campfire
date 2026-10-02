use campfire_math::Vec3;
use campfire_sim::{Capability, Position, StableId};

use crate::deliveries::delivering::Delivering;
use crate::scripts::effects::Effect;
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
    const CAPABILITY: Capability = Capability::Projectiles;
}
