use campfire_common::DeclaredName;
use campfire_declarations::{Section, SectionTable};
use campfire_script::{DataSection, DataTable};
use serde::{Deserialize, Serialize};

use crate::on_death::OnDeath;

/// A unit type's `combat` section: it holds the life pool, stays when it dies or despawns, and
/// takes damage through the mode's armor it names, whole with none. Its weapons are actions in
/// its slots.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatData {
    #[serde(default)]
    pub on_death: OnDeath,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub armor: Option<DeclaredName>,
}

impl DataSection for CombatData {
    const TABLE: DataTable = DataTable::new("A unit type's `combat`");
}

impl Section for CombatData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "combat";
}
