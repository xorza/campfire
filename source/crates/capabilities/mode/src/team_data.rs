use campfire_script::{DataSection, DataTable};
use campfire_values::TextId;
use serde::{Deserialize, Serialize};

/// A team of the manifest, as the mode's `[teams.<team>]` gives it: the message of the name
/// players see.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamData {
    pub name: TextId,
}

impl DataSection for TeamData {
    const TABLE: DataTable = DataTable::new("A team, `[teams.<team>]`");
}
