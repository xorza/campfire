use campfire_geometry::Shape;
use campfire_math::Vec3;
use campfire_sim::StableId;
use campfire_values::{TagSet, Team};

/// A unit that may be targeted: alive, with health, a position, a team, its body's shape, a point
/// for a unit with no body, and its tags. Its values are those of the moment it was looked up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LivingUnit {
    pub id: StableId,
    pub pos: Vec3,
    pub team: Team,
    pub shape: Shape,
    pub tags: TagSet,
}
