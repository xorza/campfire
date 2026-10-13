use crate::units::modifier_id::ModifierId;
use crate::units::unit_type::UnitType;

/// A unit type the mode spawns, loaded: a type of the mode's `units.toml`, or an avatar's. The
/// books hold its parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UnitTypeSetup {
    pub(crate) unit_type: UnitType,
    /// The modifier it holds from its spawn on, from itself.
    pub(crate) passive: Option<ModifierId>,
}
