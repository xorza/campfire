use std::num::NonZeroU8;

use campfire_common::DeclaredName;
use campfire_declarations::{Section, SectionTable};
use campfire_script::{DataSection, DataTable};
use serde::{Deserialize, Serialize};

/// A unit type's `spawns`: `count` units of the unit types of its package, taken in turn, that
/// leave its exit as it stands whole, or the free cell nearest its centre for a type of no exit,
/// each that dies coming back `replace_ms` later, all dying with it; when `led`, the type leads
/// them (04-capabilities/production.md, A building may keep spawns;
/// 04-capabilities/production.md, Spawns take their types in turn; 04-capabilities/production.md, A
/// unit may lead its spawns).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpawnsData {
    pub unit_types: Vec<DeclaredName>,
    pub count: NonZeroU8,
    pub replace_ms: u32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub led: bool,
}

impl DataSection for SpawnsData {
    const TABLE: DataTable = DataTable::new("A unit type's `spawns`");
}

impl Section for SpawnsData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "spawns";
}
