use std::collections::BTreeMap;

use campfire_capabilities::{AbilityData, ModifierData};
use serde::Deserialize;

/// A loadout package's `data/loadout.toml`: the abilities players choose beside their avatar's,
/// each with the ranks of the slot kind its choice fills, and the modifiers they apply.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoadoutData {
    #[serde(default)]
    pub abilities: BTreeMap<String, AbilityData>,
    #[serde(default)]
    pub modifiers: BTreeMap<String, ModifierData>,
}
