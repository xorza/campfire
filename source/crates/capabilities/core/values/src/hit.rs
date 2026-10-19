use campfire_math::{Num, Vec3};
use campfire_sim::StableId;
use serde::{Deserialize, Serialize};

/// How a delivery reached a unit, or where it ended: the projectile or area that delivered it,
/// the unit its action aimed at, if one, where, after how many meters flown, and in which
/// direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hit {
    pub delivery: Option<StableId>,
    pub target: Option<StableId>,
    pub position: Vec3,
    pub distance: Num,
    pub direction: Option<Vec3>,
}
