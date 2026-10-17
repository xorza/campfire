use std::collections::BTreeMap;

use campfire_capabilities::Release;
use campfire_common::DeclaredName;
use campfire_declare::UnitTypeData;
use serde::Deserialize;

/// The mode's `data/units.toml`: its unit types, by name.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UnitsFile {
    #[serde(deserialize_with = "Release::read_units")]
    pub units: BTreeMap<DeclaredName, UnitTypeData>,
}
