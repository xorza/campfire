use campfire_common::DeclaredName;
use campfire_declarations::{Section, SectionTable};
use campfire_script::{ApiVersion, Status};
use campfire_values::TagFilterData;
use serde::{Deserialize, Serialize};

/// A unit type's `upgrades`: the modifiers of its package its units hold while their player's
/// modifiers and their own meet each entry (04-capabilities/stats.md, A unit type takes modifiers
/// by upgrades).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UpgradesData(pub Vec<UpgradeData>);

/// One entry of `upgrades`: its `triggers`, any one of which, or with `all` every one, must be
/// held, its `conflicts`, none of which may be, the tags its unit must pass, `when`, and the
/// `modifier` it then holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpgradeData {
    pub triggers: Vec<DeclaredName>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub all: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<DeclaredName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<TagFilterData>,
    pub modifier: DeclaredName,
}

impl Section for UpgradesData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "upgrades";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}
