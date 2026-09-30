use campfire_sim::{Position, StableId};

use crate::units::team::Team;

/// A unit that may be targeted: alive, with health, a position and a team. Its values are those
/// of the moment it was looked up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LivingUnit {
    pub(crate) id: StableId,
    pub(crate) pos: Position,
    pub(crate) team: Team,
}
