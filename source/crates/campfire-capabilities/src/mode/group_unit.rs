use campfire_sim::StableId;

use crate::units::unit_type::UnitType;

/// A unit of a spawn group: its unit type, and the id a call took for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GroupUnit {
    pub(crate) unit_type: UnitType,
    pub(crate) id: StableId,
}
