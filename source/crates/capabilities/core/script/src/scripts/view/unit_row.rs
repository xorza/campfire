use campfire_common::PlayerSlot;
use campfire_geometry::Shape;
use campfire_sim::{Position, StableId};
use campfire_values::{Team, UnitTags, UnitType};

/// A unit as the view read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitRow {
    pub id: StableId,
    pub pos: Position,
    pub team: Team,
    /// Its body's shape, a point for a unit with no body.
    pub shape: Shape,
    /// Where it spawned, if it did as a unit of the mode.
    pub spawn: Option<Position>,
    pub unit_type: UnitType,
    /// The player who controls it.
    pub owner: Option<PlayerSlot>,
    /// Whether it is not dead; `combat` fills it, and the next.
    pub alive: bool,
    /// Whether it may be a target: a living unit with the life pool whose tags let it be one, by
    /// the rule `Targets` holds; combat fills it.
    pub targetable: bool,
    /// Its tags and their properties, as the core derives them.
    pub tags: UnitTags,
}
