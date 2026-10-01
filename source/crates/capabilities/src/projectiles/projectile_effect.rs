use campfire_math::Vec3;
use campfire_sim::{Position, StableId};

use crate::deliveries::delivering::Delivering;

/// A projectile a call queued: of `by`, from `from`, toward a direction or homing on a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProjectileEffect {
    pub(crate) by: Delivering,
    pub(crate) from: Position,
    pub(crate) toward: Toward,
}

/// Where a projectile flies: along a direction, or homing on a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Toward {
    Direction(Vec3),
    Unit(StableId),
}
