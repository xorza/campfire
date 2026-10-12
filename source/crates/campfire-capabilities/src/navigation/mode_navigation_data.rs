use serde::Deserialize;

use crate::scripts::data_section::DataSection;
use crate::scripts::script_api::data_table::DataTable;
use crate::values::declared_name::DeclaredName;

/// The mode's `[navigation]` section: the layers bodies move on, the first the default. A mode
/// that names none has one layer, with no name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeNavigationData {
    #[serde(default)]
    pub layers: Vec<DeclaredName>,
}

impl DataSection for ModeNavigationData {
    const TABLE: DataTable = DataTable::ModeNavigation;
}
