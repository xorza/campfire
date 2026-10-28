use campfire_common::DeclaredName;
use campfire_declarations::{Section, SectionTable};
use campfire_script::{ApiVersion, Status};
use campfire_script::{DataSection, DataTable};
use serde::{Deserialize, Serialize};

use crate::frame_point::FramePoint;

/// A unit type's `drop_off` section: the mode's player resources a worker's load of joins its
/// player's there, and the point of its frame a worker docks at to deliver, none to dock at its
/// body (exits-docks-and-selection.md, A dock has its point).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DropOffData {
    pub resources: Vec<DeclaredName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dock: Option<FramePoint>,
}

impl DataSection for DropOffData {
    const TABLE: DataTable = DataTable::new("A unit type's `drop_off`");
}

impl Section for DropOffData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "drop_off";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}
