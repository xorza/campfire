use std::collections::BTreeMap;

use campfire_capabilities::{ActionData, ModifierData};
use serde::Deserialize;
use serde::de::Error;

use crate::files::units_data::UnitTypeFile;

/// What a package holds for a match, each by its id in the package: its actions, its modifiers,
/// and its unit types. The mode, an avatar and a loadout hold the same shape.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageContent {
    #[serde(default)]
    pub actions: BTreeMap<String, ActionData>,
    #[serde(default)]
    pub modifiers: BTreeMap<String, ModifierData>,
    #[serde(default)]
    pub units: BTreeMap<String, UnitTypeFile>,
}

/// The keys of a data file's table that hold a package's content.
const KEYS: [&str; 3] = ["actions", "modifiers", "units"];

impl PackageContent {
    /// Takes the content's keys out of `table`, a data file's, and reads them; the rest of the
    /// table stays.
    pub(crate) fn take<E: Error>(table: &mut toml::Table) -> Result<PackageContent, E> {
        let content: toml::Table = KEYS
            .into_iter()
            .filter_map(|key| table.remove_entry(key))
            .collect();
        PackageContent::deserialize(toml::Value::Table(content)).map_err(E::custom)
    }
}
