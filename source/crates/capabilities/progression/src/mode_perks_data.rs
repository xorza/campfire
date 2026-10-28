use std::collections::BTreeMap;

use campfire_common::DeclaredName;
use campfire_declarations::{Section, SectionTable};
use campfire_script::{ApiVersion, Status};
use serde::{Deserialize, Serialize};

use crate::perk_data::PerkData;

/// The mode's `[perks]`: the perks its players learn, by name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModePerksData(pub BTreeMap<DeclaredName, PerkData>);

impl Section for ModePerksData {
    const TABLE: SectionTable = SectionTable::Mode;
    const KEY: &'static str = "perks";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}
