use campfire_math::{Num, Vec3};
use campfire_sim::{Position, StableId};

/// How a delivery reached a unit, or where it ended: the projectile that delivered it, the unit
/// its action aimed at, if one, where, after how many meters flown, and in which direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Hit {
    pub(crate) delivery: Option<StableId>,
    pub(crate) target: Option<StableId>,
    pub(crate) pos: Position,
    pub(crate) distance: Num,
    pub(crate) direction: Vec3,
}
