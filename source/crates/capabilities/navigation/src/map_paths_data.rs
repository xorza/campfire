use campfire_common::DeclaredName;
use campfire_declare::{Section, SectionTable};
use campfire_units::MapPoint;
use serde::{Deserialize, Serialize};

/// A map's `[[paths]]`, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MapPathsData(pub Vec<PathData>);

/// A path of the map: its name, and its waypoints in the order a unit walks them from its start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathData {
    pub name: DeclaredName,
    pub points: Vec<MapPoint>,
}

impl Section for MapPathsData {
    const TABLE: SectionTable = SectionTable::Map;
    const KEY: &'static str = "paths";
}
