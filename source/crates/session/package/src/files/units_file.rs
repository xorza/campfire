use std::collections::BTreeMap;

use campfire_capabilities::UnitTypeData;
use campfire_values::DeclaredName;
use serde::Deserialize;

/// The mode's `data/units.toml`: its unit types, by name.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UnitsFile {
    pub units: BTreeMap<DeclaredName, UnitTypeData>,
}
