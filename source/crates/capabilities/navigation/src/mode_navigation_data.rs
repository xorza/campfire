use campfire_common::DeclaredName;
use campfire_declarations::{Section, SectionTable};
use campfire_script::{ApiVersion, Status};
use campfire_script::{DataSection, DataTable};
use serde::Deserialize;

use crate::landing_data::LandingData;
use crate::moves::Moves;

/// The mode's `[navigation]` section: the layers bodies move on, the first the default, the tags
/// of the markers its units spawn at, which the map's load checks a walker can stand on, which
/// units a walker may move, and how a flier finds the point it lands at. A mode that names no
/// layer has one, with no name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeNavigationData {
    #[serde(default)]
    pub layers: Vec<DeclaredName>,
    #[serde(default)]
    pub spawn_tags: Vec<DeclaredName>,
    #[serde(default)]
    pub moves: Moves,
    #[serde(default)]
    pub landing: LandingData,
}

impl DataSection for ModeNavigationData {
    const TABLE: DataTable = DataTable::new("The mode's `[navigation]`");
}

impl Section for ModeNavigationData {
    const TABLE: SectionTable = SectionTable::Mode;
    const KEY: &'static str = "navigation";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}
