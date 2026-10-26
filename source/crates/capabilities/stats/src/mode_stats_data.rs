use std::collections::BTreeMap;

use campfire_declarations::{Section, SectionTable};
use campfire_script::{ApiVersion, Status};
use campfire_values::Stat;
use serde::Deserialize;

use crate::stat_rule::StatRule;

/// The mode's `[stats]`: every stat its units carry, those the engine reads among them, each with
/// its rule.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct ModeStatsData(pub BTreeMap<Stat, StatRule>);

impl Section for ModeStatsData {
    const TABLE: SectionTable = SectionTable::Mode;
    const KEY: &'static str = "stats";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}
