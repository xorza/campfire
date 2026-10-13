use campfire_core::{DataSection, DataTable};
use serde::Deserialize;

use crate::on_death::OnDeath;

/// A unit type's `combat` section: it holds the life pool, and stays when it dies or despawns.
/// Its weapons are actions in its slots.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatData {
    #[serde(default)]
    pub on_death: OnDeath,
}

impl DataSection for CombatData {
    const TABLE: DataTable = DataTable::Combat;
}
