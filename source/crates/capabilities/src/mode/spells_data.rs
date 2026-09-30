use std::collections::BTreeMap;

use serde::Deserialize;

use crate::abilities::ability_data::AbilityData;
use crate::stats::modifier_data::ModifierData;

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
