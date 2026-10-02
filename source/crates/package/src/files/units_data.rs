use std::collections::BTreeMap;

use campfire_capabilities::{DeclaredName, UnitTypeFile};
use serde::Deserialize;

/// The mode's `data/units.toml`: its unit types, by name.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitsData {
    pub units: BTreeMap<DeclaredName, UnitTypeFile>,
}
