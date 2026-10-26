use campfire_common::DeclaredName;
use campfire_declarations::{Section, SectionTable};
use campfire_math::Num;
use campfire_script::{ApiVersion, Status};
use campfire_units::MapPoint;
use serde::{Deserialize, Serialize};

/// A map's `[navigation]`: the size of the square cells routes are planned on, in meters, and its
/// walls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapNavigationData {
    pub cell: Num,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub walls: Vec<WallData>,
}

/// A wall, `[[navigation.walls]]`: on `layer`, the mode's first when it names none, the polygon of
/// `points`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WallData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer: Option<DeclaredName>,
    pub points: Vec<MapPoint>,
}

impl Section for MapNavigationData {
    const TABLE: SectionTable = SectionTable::Map;
    const KEY: &'static str = "navigation";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}
