use campfire_common::DeclaredName;
use campfire_math::Num;
use campfire_units::MapPoint;
use serde::{Deserialize, Serialize};

/// A location a bot's script names: the point of the map's marker of a name, which a map holds
/// once, `{ marker = "<name>" }`; or a point, `[x, z]` on a planar map and `[x, y, z]` on a spatial
/// one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Location {
    Marker(MarkerLocation),
    At(MapPoint),
}

/// A point of the ground a bot's script names: a location; or, for a build's place, the nearest
/// point round one where the build passes its placement as the order goes out, `{ near =
/// <location>, gap = <meters>, steps = <count> }`, on the grid of `gap` within `steps` gaps, at
/// most 128 (04-capabilities/control.md, A bot finds a building's place as the build tests it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GroundLocation {
    At(Location),
    Near(NearLocation),
}

/// The search a build's place names: the location it starts from, the gap of its grid, and how
/// many gaps it reaches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NearLocation {
    pub near: Location,
    pub gap: Num,
    pub steps: u32,
}

/// The marker a location names, by its name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkerLocation {
    pub marker: DeclaredName,
}
