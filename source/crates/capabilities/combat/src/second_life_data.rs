use campfire_common::DeclaredName;
use campfire_declarations::{Section, SectionTable};
use campfire_script::{ApiVersion, DataSection, DataTable, Status};
use serde::{Deserialize, Serialize};

/// A unit type's `second_life`: the modifier of its package its unit holds once the first damage
/// that would kill it leaves it the least life, its life then whole, and the damage kinds that
/// kill it through it, as Zero Hour's `UndeadBody` lives twice but against `UNRESISTABLE` damage
/// (defenses-and-airfields.md, A type may live twice).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecondLifeData {
    pub modifier: DeclaredName,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lethal: Vec<DeclaredName>,
}

impl DataSection for SecondLifeData {
    const TABLE: DataTable = DataTable::new("A unit type's `second_life`");
}

impl Section for SecondLifeData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "second_life";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}
