use std::collections::BTreeMap;

use campfire_math::{Num, Vec3};
use campfire_sim::Position;
use serde::Deserialize;

use crate::values::scalar::Scalar;

/// The mode's `map/map.toml`: its lanes, where each team's heroes spawn, the structures that
/// stand from the start, and where neutral units spawn.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapData {
    /// Each runs from the first team's end to the second's.
    #[serde(default)]
    pub lanes: Vec<LaneData>,
    /// Where each team's heroes spawn, by team name.
    pub spawns: BTreeMap<String, GroundPoint>,
    #[serde(default)]
    pub structures: Vec<StructureData>,
    #[serde(default)]
    pub neutral_spawns: Vec<NeutralSpawnData>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaneData {
    pub name: String,
    pub points: Vec<GroundPoint>,
}

/// A unit that stands on the map from the start, of a team, and on a lane if it guards one.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructureData {
    pub unit_type: String,
    pub team: String,
    pub lane: Option<String>,
    pub pos: GroundPoint,
}

/// Where the mode script spawns a neutral unit, as `ctx.map.neutral_spawns` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NeutralSpawnData {
    pub unit_type: String,
    pub pos: GroundPoint,
}

/// A point on the ground plane, `[x, z]` in meters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct GroundPoint(pub [Scalar; 2]);

impl GroundPoint {
    /// The point at height 0; `None` beyond the world's bound.
    pub fn position(self) -> Option<Position> {
        let [x, z] = self.0.map(Scalar::to_num);
        Position::new(Vec3::new(x?, Num::ZERO, z?))
    }
}
