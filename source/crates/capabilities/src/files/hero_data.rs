use std::collections::BTreeMap;

use serde::Deserialize;

use crate::abilities::ability_data::AbilityData;
use crate::combat::combat_data::CombatData;
use crate::stats::modifier_data::ModifierData;
use crate::stats::stats_data::StatsData;

/// A hero package's `data/hero.toml`. A hero carries the tag `hero`, and stays when it dies.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeroData {
    pub name: String,
    pub role: String,
    /// What its abilities cost.
    pub resource: ResourceKind,
    /// The modifier it always carries.
    pub passive: Option<String>,
    /// Its four abilities; the last is the ultimate.
    pub slots: [String; 4],
    pub combat: CombatData,
    pub stats: StatsData,
    #[serde(default)]
    pub abilities: BTreeMap<String, AbilityData>,
    #[serde(default)]
    pub modifiers: BTreeMap<String, ModifierData>,
}

impl HeroData {
    /// The ranks of the ability in `slot`: 5 for a basic ability, 3 for the ultimate, the last.
    pub const fn slot_ranks(slot: usize) -> u8 {
        if slot == 3 { 3 } else { 5 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Mana,
    Energy,
}
