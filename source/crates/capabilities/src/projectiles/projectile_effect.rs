use campfire_math::Vec3;
use campfire_sim::{Position, StableId};

use crate::actions::action_book::ActionId;

/// A projectile a call queued: `source`'s, from `from`, of `action` at `rank`, toward a
/// direction or homing on a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProjectileEffect {
    pub(crate) source: StableId,
    pub(crate) from: Position,
    pub(crate) action: ActionId,
    pub(crate) rank: u8,
    pub(crate) toward: Toward,
}

/// Where a projectile flies: along a direction, or homing on a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Toward {
    Direction(Vec3),
    Unit(StableId),
}
