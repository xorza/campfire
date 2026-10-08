use crate::mode::slot_action::SlotAction;
use crate::mode::unit_kit::UnitKit;
use crate::units::modifier_id::ModifierId;
use crate::units::unit_type::UnitType;

/// A unit type the mode spawns, loaded, with its kit: a type of the mode's `units.toml`, or an
/// avatar's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnitTypeSetup {
    pub(crate) unit_type: UnitType,
    pub(crate) kit: UnitKit,
    /// Its actions, kind after kind in the mode's order.
    pub(crate) actions: Vec<SlotAction>,
    /// The modifier it holds from its spawn on, from itself.
    pub(crate) passive: Option<ModifierId>,
}
