use campfire_script::{DataSection, DataTable};
use campfire_values::MessageId;
use serde::Deserialize;

/// A team of the manifest, as the mode's `[teams.<team>]` gives it: the message of the name
/// players see.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamData {
    pub name: MessageId,
}

impl DataSection for TeamData {
    const TABLE: DataTable = DataTable::Team;
}
