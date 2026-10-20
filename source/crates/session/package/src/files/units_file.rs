use std::collections::BTreeMap;

use campfire_capabilities::Release;
use campfire_codec::{TomlError, TomlValue};
use campfire_common::DeclaredName;
use campfire_declarations::{UnitDefaults, UnitTypeData};
use serde::Deserialize;

/// The mode's `data/units.toml`: its unit types, by name, kept whole until the mode's defaults lie
/// under them.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UnitsFile {
    units: TomlValue,
}

impl UnitsFile {
    /// Its unit types, each read once `defaults` lie under its sections.
    pub(crate) fn read(
        self,
        defaults: &UnitDefaults,
    ) -> Result<BTreeMap<DeclaredName, UnitTypeData>, TomlError> {
        Release::read_units(self.units, defaults)
    }
}
