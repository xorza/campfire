use std::collections::BTreeMap;

use campfire_math::{Num, Vec3};
use campfire_sim::Position;
use serde::Deserialize;

use crate::mode::error::ModeError;
use crate::values::bounds::Bounds;
use crate::values::grid::Grid;
use crate::values::scalar::Scalar;

/// The mode's `map/map.toml`: its bounds, its grid, its paths, where each team's avatars spawn,
/// the structures that stand from the start, and where neutral units spawn.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapData {
    pub bounds: Bounds,
    /// The cells vision reveals, over the bounds; a mode that declares `vision` has one.
    pub grid: Option<GridData>,
    /// The cells units plan routes over, over the bounds; a mode that declares `navigation` has
    /// one.
    pub navigation: Option<GridData>,
    /// Each runs from the first team's end to the second's.
    #[serde(default)]
    pub paths: Vec<PathData>,
    /// Where each team's avatars spawn, by team name.
    pub spawns: BTreeMap<String, GroundPoint>,
    #[serde(default)]
    pub structures: Vec<StructureData>,
    #[serde(default)]
    pub neutral_spawns: Vec<NeutralSpawnData>,
}

/// A map's `[grid]` or `[navigation]`: the size of the square cells of a grid over its bounds, in
/// meters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GridData {
    pub cell: Scalar,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathData {
    pub name: String,
    pub points: Vec<GroundPoint>,
}

/// A unit that stands on the map from the start, of a team, and on a path if it guards one.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructureData {
    pub unit_type: String,
    pub team: String,
    pub path: Option<String>,
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

impl MapData {
    /// The vision grid over the bounds, if the map has one; an error unless its cell is positive
    /// and at most the world's bound, and it has at most 2²² cells.
    pub fn grid(&self) -> Result<Option<Grid>, ModeError> {
        self.grid_of(self.grid)
    }

    /// The pathing grid's cells over the bounds, if the map has them; an error as for `grid`.
    pub fn pathing(&self) -> Result<Option<Grid>, ModeError> {
        self.grid_of(self.navigation)
    }

    fn grid_of(&self, data: Option<GridData>) -> Result<Option<Grid>, ModeError> {
        let Some(data) = data else {
            return Ok(None);
        };
        let grid = data
            .cell
            .to_num()
            .and_then(|cell| Grid::new(cell, self.bounds));
        grid.map(Some).ok_or(ModeError::Grid)
    }
}

impl PathData {
    /// Whether a waypoint of the path is closer than `reach` to `at` on the ground plane,
    /// exactly; one at `reach` is not closer. Its points passed the mode's check.
    pub fn has_point_within(&self, at: Position, reach: Num) -> bool {
        let reach = u128::from(reach.to_bits().unsigned_abs());
        self.points.iter().any(|point| {
            let point = point.position().expect("the mode's check passed");
            point.ground_offset(at).length_squared_bits() < reach * reach
        })
    }
}

impl GroundPoint {
    /// The point at height 0; `None` beyond the world's bound.
    pub fn position(self) -> Option<Position> {
        let [x, z] = self.0.map(Scalar::to_num);
        Position::new(Vec3::new(x?, Num::ZERO, z?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn num(value: i64) -> Num {
        Num::from_int(value).unwrap()
    }

    #[test]
    fn a_path_has_a_point_within_a_reach_exactly() {
        let path = PathData {
            name: "lane".to_owned(),
            points: [[0, 0], [10, 0]]
                .map(|[x, z]| GroundPoint([Scalar::Int(x), Scalar::Int(z)]))
                .to_vec(),
        };
        let at = |x: i64, z: i64| Position::new(Vec3::new(num(x), Num::ZERO, num(z))).unwrap();
        // (13, 4) is 5 m from (10, 0); (5, 1), on the segment's side, is 1 m from it but 5.1 m
        // from either point.
        assert!(!path.has_point_within(at(13, 4), num(5)));
        assert!(path.has_point_within(at(13, 4), num(5) + Num::EPSILON));
        assert!(!path.has_point_within(at(5, 1), num(5)));
    }
}
