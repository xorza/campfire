use std::collections::BTreeMap;

use serde::Deserialize;

use crate::actions::action_data::ActionData;
use crate::books::unit_type_file::UnitTypeFile;
use crate::items::item_data::ItemData;
use crate::stats::modifier_data::ModifierData;
use crate::values::declared_name::DeclaredName;

/// What a package holds for a match, each by its id in the package: its actions, its modifiers,
/// its unit types and its item types. The mode, an avatar and a loadout hold the same shape, and
/// the load lets only the mode hold item types.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageContent {
    #[serde(default)]
    pub actions: BTreeMap<DeclaredName, ActionData>,
    #[serde(default)]
    pub modifiers: BTreeMap<DeclaredName, ModifierData>,
    #[serde(default)]
    pub units: BTreeMap<DeclaredName, UnitTypeFile>,
    #[serde(default)]
    pub items: BTreeMap<DeclaredName, ItemData>,
}

impl PackageContent {
    /// The keys of a data file's table that hold a package's content: its fields' names.
    pub const KEYS: [&'static str; 4] = ["actions", "modifiers", "units", "items"];
}

#[cfg(test)]
mod tests;
