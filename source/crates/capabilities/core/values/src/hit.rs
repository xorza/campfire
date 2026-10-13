use campfire_math::{Num, Vec3};
use campfire_sim::{Position, StableId};

/// How a delivery reached a unit, or where it ended: the projectile or area that delivered it,
/// the unit its action aimed at, if one, where, after how many meters flown, and in which
/// direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    pub delivery: Option<StableId>,
    pub target: Option<StableId>,
    pub pos: Position,
    pub distance: Num,
    pub direction: Option<Vec3>,
}
