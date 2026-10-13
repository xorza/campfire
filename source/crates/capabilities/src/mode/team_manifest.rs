use serde::Deserialize;

use crate::values::declared_name::DeclaredName;

/// A playing team as a mode's manifest declares it: its name, and how many player slots it has.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamManifest {
    pub name: DeclaredName,
    pub slots: u32,
}
