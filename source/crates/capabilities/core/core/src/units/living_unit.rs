use campfire_geometry::Shape;
use campfire_sim::{Position, StableId};

use crate::units::tag_set::TagSet;
use crate::units::team::Team;

/// A unit that may be targeted: alive, with health, a position, a team, its body's shape, a point
/// for a unit with no body, and its tags. Its values are those of the moment it was looked up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LivingUnit {
    pub id: StableId,
    pub pos: Position,
    pub team: Team,
    pub shape: Shape,
    pub tags: TagSet,
}
