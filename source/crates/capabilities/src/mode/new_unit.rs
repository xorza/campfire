use campfire_sim::StableId;

use crate::units::unit_type::UnitType;

/// A unit a mode call spawns, `NewUnit` in scripts: it spawns when the call ends, so the call
/// can give it to `grant`, but reads none of its fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NewUnit {
    pub(crate) id: StableId,
    pub(crate) unit_type: UnitType,
}
