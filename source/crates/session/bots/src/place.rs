use campfire_common::DeclaredName;
use campfire_units::MapPoint;
use serde::{Deserialize, Serialize};

/// A place a bot's script names: the point of the map's marker of a name, which a map holds once,
/// `{ marker = "<name>" }`; or a point, `[x, z]` on a planar map and `[x, y, z]` on a spatial
/// one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Place {
    Marker(MarkerPlace),
    At(MapPoint),
}

/// The marker a place names, by its name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkerPlace {
    pub marker: DeclaredName,
}
