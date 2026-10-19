use campfire_common::DeclaredName;
use campfire_declare::{Section, SectionTable};
use campfire_script::{DataSection, DataTable};
use serde::Deserialize;

/// The mode's `victory` section: the tag of the units by which a player stays in play, and what
/// a player's defeat does to its other units (04-capabilities/games.md, Victory by the last unit
/// of a tag).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VictoryData {
    pub counts: DeclaredName,
    #[serde(default)]
    pub defeat: Defeat,
}

/// What a player's defeat does to the units it still holds: they live on, or they die.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Defeat {
    #[default]
    Keep,
    KillAll,
}

impl DataSection for VictoryData {
    const TABLE: DataTable = DataTable::new("The mode's `[victory]`");
}

impl Section for VictoryData {
    const TABLE: SectionTable = SectionTable::Mode;
    const KEY: &'static str = "victory";
}
