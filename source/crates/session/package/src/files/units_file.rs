use std::collections::BTreeMap;

use campfire_capabilities::Release;
use campfire_codec::{TomlError, TomlTable, TomlValue};
use campfire_common::DeclaredName;
use campfire_declarations::{UnitDefaults, UnitTypeData};
use campfire_values::PackagePath;
use serde::{Deserialize, Serialize};

/// The mode's `data/units.toml`: its unit types, by name, kept whole until the mode's defaults lie
/// under them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitsFile {
    units: TomlValue,
}

impl UnitsFile {
    /// Where a mode's package holds it.
    pub fn path() -> PackagePath {
        PackagePath::parse("data/units.toml").expect("a package path")
    }

    /// The file of `units`, each unit type's table by its name, as a tool writes it.
    pub fn of(units: &BTreeMap<DeclaredName, TomlTable>) -> Self {
        Self {
            units: TomlValue::of(units),
        }
    }

    /// Its unit types, each read once `defaults` lie under its sections.
    pub(crate) fn read(
        self,
        defaults: &UnitDefaults,
    ) -> Result<BTreeMap<DeclaredName, UnitTypeData>, TomlError> {
        Release::read_units(self.units, defaults)
    }
}
