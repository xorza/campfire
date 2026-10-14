use campfire_script::{DataSection, DataTable};
use campfire_values::DeclaredName;
use serde::Deserialize;

/// The mode's `[navigation]` section: the layers bodies move on, the first the default, and the
/// tags of the markers its units spawn at, which the map's load checks a walker can stand on. A
/// mode that names no layer has one, with no name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeNavigationData {
    #[serde(default)]
    pub layers: Vec<DeclaredName>,
    #[serde(default)]
    pub spawn_tags: Vec<DeclaredName>,
}

impl DataSection for ModeNavigationData {
    const TABLE: DataTable = DataTable::ModeNavigation;
}
