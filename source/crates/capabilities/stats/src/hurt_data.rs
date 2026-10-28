use campfire_common::DeclaredName;
use campfire_declarations::{Section, SectionTable};
use campfire_script::{ApiVersion, Status};
use campfire_values::Share;
use serde::{Deserialize, Serialize};

/// A unit type's `hurt`: the modifier of its package its units hold while their life is at most
/// the share `below` of its maximum, as Zero Hour's movers take their locomotor's damaged numbers
/// once really damaged (04-capabilities/stats.md, A hurt type may hold a modifier).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HurtData {
    pub below: Share,
    pub modifier: DeclaredName,
}

impl Section for HurtData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "hurt";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}
