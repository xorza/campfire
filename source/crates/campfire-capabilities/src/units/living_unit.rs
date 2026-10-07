use campfire_sim::{Position, StableId};

use crate::units::tag_set::TagSet;
use crate::units::team::Team;
use crate::values::shape::Shape;

/// A unit that may be targeted: alive, with health, a position, a team, its body's shape, a point
/// for a unit with no body, and its tags. Its values are those of the moment it was looked up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LivingUnit {
    pub(crate) id: StableId,
    pub(crate) pos: Position,
    pub(crate) team: Team,
    pub(crate) shape: Shape,
    pub(crate) tags: TagSet,
}
