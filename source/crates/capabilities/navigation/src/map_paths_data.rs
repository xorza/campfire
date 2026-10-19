use campfire_common::DeclaredName;
use campfire_declare::{Section, SectionTable};
use campfire_units::MapPoint;
use serde::{Deserialize, Serialize};

use crate::path_walk::PathWalk;

/// A map's `[[paths]]`, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MapPathsData(pub Vec<PathData>);

/// A path of the map: its name, its waypoints in the order a unit walks them from its start, and
/// how its walkers go past the last, `once` when not given.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathData {
    pub name: DeclaredName,
    pub points: Vec<MapPoint>,
    #[serde(default, skip_serializing_if = "is_once")]
    pub walk: PathWalk,
}

impl Section for MapPathsData {
    const TABLE: SectionTable = SectionTable::Map;
    const KEY: &'static str = "paths";
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's `skip_serializing_if` passes the field by reference"
)]
const fn is_once(walk: &PathWalk) -> bool {
    matches!(walk, PathWalk::Once)
}
