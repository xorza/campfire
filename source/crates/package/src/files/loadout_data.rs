use std::collections::BTreeMap;

use campfire_capabilities::{AbilityData, ModifierData};
use serde::Deserialize;

/// A loadout package's `data/loadout.toml`: the abilities players pick beside their avatar's, each of
/// one rank, and the modifiers they apply.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoadoutData {
    #[serde(default)]
    pub abilities: BTreeMap<String, AbilityData>,
    #[serde(default)]
    pub modifiers: BTreeMap<String, ModifierData>,
}

impl LoadoutData {
    /// The ranks of every entry.
    pub const RANKS: u8 = 1;
}
