use std::num::NonZeroU32;

use campfire_declarations::{Section, SectionTable};
use campfire_script::{ApiVersion, DataSection, DataTable, Status};
use serde::{Deserialize, Serialize};

/// A unit type's `rides` section, which lets its units enter containers: the room each takes of
/// a container that counts its riders' sizes, at least one (transports.md, A container says who
/// rides).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RidesData {
    pub size: NonZeroU32,
}

impl DataSection for RidesData {
    const TABLE: DataTable = DataTable::new("A unit type's `rides`");
}

impl Section for RidesData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "rides";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}
