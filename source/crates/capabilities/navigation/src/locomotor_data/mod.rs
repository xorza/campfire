use campfire_declarations::{Section, SectionTable};
use campfire_script::{ApiVersion, DataSection, DataTable, Status};
use serde::{Deserialize, Serialize};

use crate::zero_hour::zero_hour_locomotor::ZeroHourLocomotor;

/// A unit type's `locomotor` section: the movement model its units walk by in place of the
/// engine's step, which keeps a velocity (driving.md, A type moves by its movement model): Zero
/// Hour's locomotor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum LocomotorData {
    ZeroHour(ZeroHourLocomotor),
}

impl DataSection for LocomotorData {
    const TABLE: DataTable = DataTable::new("A unit type's `locomotor`");
}

impl Section for LocomotorData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "locomotor";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}

#[cfg(test)]
mod tests;
