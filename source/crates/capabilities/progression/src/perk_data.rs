use std::num::NonZeroU32;

use campfire_common::DeclaredName;
use campfire_script::{DataSection, DataTable};
use campfire_stats::Level;
use serde::{Deserialize, Serialize};

/// A perk of the mode's `[perks.<name>]`, which a player learns for its points or the mode's script
/// grants (04-capabilities/progression.md, A science is a perk of a player): its `cost` in points,
/// none for a perk no player learns, as a science of cost 0; the perks it `requires`; the `level`
/// of the player track `track` it needs; and the player modifier of the mode's package its player
/// holds once it has it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PerkData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<NonZeroU32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<DeclaredName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track: Option<DeclaredName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<Level>,
    pub modifier: DeclaredName,
}

impl DataSection for PerkData {
    const TABLE: DataTable = DataTable::new("A perk, `[perks.<name>]`");
}
