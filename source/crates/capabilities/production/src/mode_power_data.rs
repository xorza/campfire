use campfire_actions::ActionKind;
use campfire_common::DeclaredName;
use campfire_declarations::{Section, SectionTable};
use campfire_script::{ApiVersion, DataSection, DataTable, Status};
use campfire_values::{Share, Stat};
use serde::Deserialize;

/// The mode's `[power]`: the unit stat whose value a unit gives above 0 and takes below, the
/// modifier its player holds while short, none for no effect, and the speed of a short player's
/// production, none for none slowed (power.md, Power is production's; power.md, A unit's power is
/// a stat).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModePowerData {
    pub stat: Stat,
    #[serde(default)]
    pub short: Option<DeclaredName>,
    #[serde(default)]
    pub speed: Option<PowerSpeedData>,
}

/// The mode's `[power] speed`: a short player's speed `1 − (1 − share) × penalty`, at least
/// `min` and at most `max`; the share of a player that gives and takes no power; and the kinds
/// of action whose time the speed changes (power.md, The speed by the ratio; power.md, What
/// power slows is the mode's).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PowerSpeedData {
    pub penalty: Share,
    pub min: Share,
    pub max: Share,
    pub empty: EmptyPower,
    pub slows: Vec<ActionKind>,
}

/// The share of its use a player gives that gives and takes no power: all of it, as OpenRA's and
/// Red Alert's, or none, as Zero Hour's `Energy::getEnergySupplyRatio`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmptyPower {
    Full,
    None,
}

impl DataSection for ModePowerData {
    const TABLE: DataTable = DataTable::new("The mode's `[power]`");
}

impl DataSection for PowerSpeedData {
    const TABLE: DataTable = DataTable::new("The mode's `[power] speed`");
}

impl Section for ModePowerData {
    const TABLE: SectionTable = SectionTable::Mode;
    const KEY: &'static str = "power";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}
