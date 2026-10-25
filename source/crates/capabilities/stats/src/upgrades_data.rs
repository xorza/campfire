use campfire_common::DeclaredName;
use campfire_declarations::{Section, SectionTable};
use campfire_values::TagFilterData;
use serde::Deserialize;

/// A unit type's `upgrades`: the modifiers of its package its units hold while their player's
/// modifiers and their own meet each entry (upgrades.md, A unit type takes modifiers by upgrades).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct UpgradesData(pub Vec<UpgradeData>);

/// One entry of `upgrades`: its `triggers`, any one of which, or with `all` every one, must be
/// held, its `conflicts`, none of which may be, the tags its unit must pass, `when`, and the
/// `modifier` it then holds.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpgradeData {
    pub triggers: Vec<DeclaredName>,
    #[serde(default)]
    pub all: bool,
    #[serde(default)]
    pub conflicts: Vec<DeclaredName>,
    pub when: Option<TagFilterData>,
    pub modifier: DeclaredName,
}

impl Section for UpgradesData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "upgrades";
}
