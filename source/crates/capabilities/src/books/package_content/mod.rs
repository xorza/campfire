use std::collections::BTreeMap;

use campfire_actions::ActionData;
use campfire_common::DeclaredName;
use campfire_items::ItemData;
use campfire_stats::ModifierData;
use serde::Deserialize;

use crate::books::unit_type_data::UnitTypeData;

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
    pub units: BTreeMap<DeclaredName, UnitTypeData>,
    #[serde(default)]
    pub items: BTreeMap<DeclaredName, ItemData>,
}

impl PackageContent {
    /// The keys of a data file's table that hold a package's content: its fields' names.
    pub const KEYS: [&'static str; 4] = ["actions", "modifiers", "units", "items"];
}

#[cfg(test)]
mod tests;
