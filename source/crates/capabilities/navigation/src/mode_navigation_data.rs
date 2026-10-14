use campfire_script::{DataSection, DataTable};
use campfire_values::DeclaredName;
use serde::Deserialize;

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
