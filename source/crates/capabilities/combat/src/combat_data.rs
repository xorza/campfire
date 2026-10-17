use campfire_common::DeclaredName;
use campfire_declare::{Section, SectionTable};
use campfire_script::{DataSection, DataTable};
use serde::Deserialize;

use crate::on_death::OnDeath;

/// A unit type's `combat` section: it holds the life pool, stays when it dies or despawns, and
/// takes damage through the mode's armor it names, whole with none. Its weapons are actions in
/// its slots.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatData {
    #[serde(default)]
    pub on_death: OnDeath,
    pub armor: Option<DeclaredName>,
}

impl DataSection for CombatData {
    const TABLE: DataTable = DataTable::Combat;
}

impl Section for CombatData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "combat";
}
