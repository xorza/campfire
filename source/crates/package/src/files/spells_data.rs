use std::collections::BTreeMap;

use campfire_capabilities::{AbilityData, ModifierData};
use serde::Deserialize;

/// A spells package's `data/spells.toml`: the abilities players pick beside their hero's, each of
/// one rank, and the modifiers they apply.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpellsData {
    #[serde(default)]
    pub abilities: BTreeMap<String, AbilityData>,
    #[serde(default)]
    pub modifiers: BTreeMap<String, ModifierData>,
}

impl SpellsData {
    /// The ranks of every spell.
    pub const RANKS: u8 = 1;
}
