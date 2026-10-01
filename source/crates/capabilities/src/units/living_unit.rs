use campfire_math::Num;
use campfire_sim::{Position, StableId};

use crate::units::tag_set::TagSet;
use crate::units::team::Team;

/// A unit that may be targeted: alive, with health, a position, a team, its body's radius, 0 for
/// a unit with no body, and its tags. Its values are those of the moment it was looked up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LivingUnit {
    pub(crate) id: StableId,
    pub(crate) pos: Position,
    pub(crate) team: Team,
    pub(crate) radius: Num,
    pub(crate) tags: TagSet,
}
