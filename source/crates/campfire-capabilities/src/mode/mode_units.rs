use crate::mode::loadout_setup::LoadoutSetup;
use crate::mode::unit_type_setup::UnitTypeSetup;
use crate::values::name_list::NameList;

/// The mode's unit types that stand, its avatars' among them, its avatars by their packages'
/// names, in the order the mode depends on them, and its loadout's entries.
#[derive(Debug, Default)]
pub struct ModeUnits {
    pub(crate) unit_types: Vec<UnitTypeSetup>,
    pub(crate) avatars: NameList,
    pub(crate) loadout: LoadoutSetup,
}
