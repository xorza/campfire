use std::num::NonZeroU8;

use campfire_declare::PerType;
use campfire_script::{DataSection, DataTable};
use campfire_sim::Capability;
use serde::Deserialize;

/// A unit type's `production` section: the most entries its train queue holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionData {
    pub queue: NonZeroU8,
}

impl PerType for ProductionData {
    const OWNER: Option<Capability> = Some(Capability::Production);
    const SCRIPTS_ONLY: bool = false;
}

impl DataSection for ProductionData {
    const TABLE: DataTable = DataTable::Production;
}
