use campfire_common::PlayerSlot;
use campfire_sim::{Position, StableId};

use crate::geometry::shape::Shape;
use crate::units::engine_tag::EngineTag;
use crate::units::team::Team;
use crate::units::unit_tags::UnitTags;
use crate::units::unit_type::UnitType;

/// A unit as the view read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UnitRow {
    pub(crate) id: StableId,
    pub(crate) pos: Position,
    pub(crate) team: Team,
    /// Its body's shape, a point for a unit with no body.
    pub(crate) shape: Shape,
    /// Where it spawned, if it did as a unit of the mode.
    pub(crate) spawn: Option<Position>,
    pub(crate) unit_type: Option<UnitType>,
    /// The player who controls it.
    pub(crate) owner: Option<PlayerSlot>,
    /// Whether it is not dead; `combat` fills it, and the next.
    pub(crate) alive: bool,
    /// Whether it may be a target: a living unit with the life pool whose tags let it be one, by
    /// the rule `Targets` holds; combat fills it.
    pub(crate) targetable: bool,
    /// Its tags and their properties, as the core derives them.
    pub(crate) tags: UnitTags,
}

impl UnitRow {
    pub(crate) fn is_avatar(&self) -> bool {
        self.tags.tags.contains(EngineTag::Avatar.tag())
    }
}
