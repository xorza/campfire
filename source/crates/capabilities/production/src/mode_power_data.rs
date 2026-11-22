use campfire_actions::ActionKind;
use campfire_common::DeclaredName;
use campfire_declarations::{PatchError, Patched, Section, SectionTable};
use campfire_script::{ApiVersion, DataSection, DataTable, Status};
use campfire_values::{Share, Stat};
use serde::{Deserialize, Deserializer, Serialize};

/// The mode's `[power]`: the unit stat whose value a unit gives above 0 and takes below, the
/// modifier its player holds while short, none for no effect, and the speed of a short player's
/// production, none for none slowed (04-capabilities/production.md, Power is production's;
/// 04-capabilities/production.md, A unit's power is a stat).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModePowerData {
    pub stat: Stat,
    pub short: Option<DeclaredName>,
    pub speed: Option<PowerSpeedData>,
}

/// The mode's `[power]` as a tool writes it in part, over the rules package's it lies on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModePowerPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stat: Option<Stat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short: Option<DeclaredName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<PowerSpeedData>,
}

/// The mode's `[power] speed`: a short player's speed `1 − (1 − share) × penalty`, at least `min`
/// and at most `max`; the share of a player that gives and takes no power; and the kinds of action
/// whose time the speed changes (04-capabilities/production.md, The speed by the ratio;
/// 04-capabilities/production.md, What power slows is the mode's).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmptyPower {
    Full,
    None,
}

impl Patched for ModePowerData {
    type Patch = ModePowerPatch;

    fn from_patch(patch: ModePowerPatch) -> Result<Self, PatchError> {
        Ok(Self {
            stat: patch.stat.ok_or(PatchError::Missing("stat"))?,
            short: patch.short,
            speed: patch.speed,
        })
    }
}

impl<'de> Deserialize<'de> for ModePowerData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::read_patch(deserializer)
    }
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
