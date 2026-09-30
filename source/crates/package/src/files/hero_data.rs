use std::collections::BTreeMap;

use campfire_capabilities::{
    AbilityData, CombatData, DeclaredName, ModifierData, StatsData, VisionData,
};
use serde::Deserialize;

/// A hero package's `data/hero.toml`. A hero carries the tag `hero`, and stays when it dies.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeroData {
    pub name: String,
    pub role: String,
    /// What its abilities cost: one of the mode's resources.
    pub resource: DeclaredName,
    /// The modifier it always carries.
    pub passive: Option<String>,
    /// Its four abilities; the last is the ultimate.
    pub slots: [String; 4],
    pub combat: CombatData,
    pub stats: StatsData,
    /// How far it sees; without it, a hero reveals nothing to its team.
    pub vision: Option<VisionData>,
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
