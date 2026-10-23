use campfire_common::DeclaredName;
use serde::{Deserialize, Serialize};

/// A playing team as a mode's manifest declares it: its name, and how many player slots it has.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamManifest {
    pub name: DeclaredName,
    pub player_slots: u32,
}
