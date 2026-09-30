use std::collections::BTreeMap;

use campfire_math::{Num, U256, Vec3};
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

/// A map's `[grid]`: the size of its square cells, in meters.
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
    /// The grid over the bounds, if the map has one; an error unless its cell is positive and at
    /// most the world's bound, and it has at most 2²² cells.
    pub fn grid(&self) -> Result<Option<Grid>, ModeError> {
        let Some(data) = self.grid else {
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
    /// Whether the path, its waypoints joined by straight segments on the ground plane, comes
    /// closer than `reach` to `at`, exactly; touching at `reach` is not closer. Its points passed
    /// the mode's check.
    pub fn comes_within(&self, at: Position, reach: Num) -> bool {
        let point = |ground: &GroundPoint| {
            let pos = ground.position().expect("the mode's check passed").get();
            [pos.x, pos.z].map(|value| i128::from(value.to_bits()))
        };
        let at = at.get();
        let at = [at.x, at.z].map(|value| i128::from(value.to_bits()));
        let reach = u128::from(reach.to_bits().unsigned_abs());
        let square = |v: [i128; 2]| (v[0] * v[0] + v[1] * v[1]).cast_unsigned();
        let within = |square: u128| square < reach * reach;
        if let [only] = &self.points[..] {
            let only = point(only);
            return within(square([at[0] - only[0], at[1] - only[1]]));
        }
        self.points.windows(2).any(|pair| {
            let [a, b] = [point(&pair[0]), point(&pair[1])];
            let along = [b[0] - a[0], b[1] - a[1]];
            let from_a = [at[0] - a[0], at[1] - a[1]];
            let length = square(along);
            let projection = from_a[0] * along[0] + from_a[1] * along[1];
            if projection <= 0 {
                return within(square(from_a));
            }
            if projection.cast_unsigned() >= length {
                return within(square([at[0] - b[0], at[1] - b[1]]));
            }
            // The nearest point lies within the segment: its squared distance is
            // |from_a|² − projection² / length, compared times `length`.
            let far = U256::product(square(from_a), length);
            let near = U256::product(reach * reach, length)
                .checked_add(U256::product(
                    projection.cast_unsigned(),
                    projection.cast_unsigned(),
                ))
                .expect("points within the world's bound");
            far < near
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

    fn path(points: &[[i64; 2]]) -> PathData {
        PathData {
            name: "lane".to_owned(),
            points: points
                .iter()
                .map(|&[x, z]| GroundPoint([Scalar::Int(x), Scalar::Int(z)]))
                .collect(),
        }
    }

    fn at(x: i64, z: i64) -> Position {
        Position::new(Vec3::new(num(x), Num::ZERO, num(z))).unwrap()
    }

    fn num(value: i64) -> Num {
        Num::from_int(value).unwrap()
    }

    #[test]
    fn a_path_comes_within_a_reach_of_a_point_exactly() {
        let e = Num::EPSILON;
        // Along x from 0 to 10: (5, 1) is 1 m off it; (−3, 4) and (13, 4) are 5 m from an end,
        // before its start and past its end.
        let straight = path(&[[0, 0], [10, 0]]);
        for (point, distance) in [(at(5, 1), 1), (at(-3, 4), 5), (at(13, 4), 5)] {
            assert!(
                !straight.comes_within(point, num(distance)),
                "{point:?} touches"
            );
            assert!(
                straight.comes_within(point, num(distance) + e),
                "{point:?} within"
            );
        }
        // From (0, 0) to (8, 6), 10 m: (1, 7) projects halfway, onto (4, 3), √(3² + 4²) = 5 m
        // away; a second segment back to (8, 20) passes farther.
        let slant = path(&[[0, 0], [8, 6], [8, 20]]);
        assert!(!slant.comes_within(at(1, 7), num(5)));
        assert!(slant.comes_within(at(1, 7), num(5) + e));
        // A path of one point is that point: (5, 6) is 5 m from (2, 2).
        let only = path(&[[2, 2]]);
        assert!(!only.comes_within(at(5, 6), num(5)));
        assert!(only.comes_within(at(5, 6), num(5) + e));
        // Across the whole world, 2²⁰ m, whose products pass 128 bits: (0, 1) is 1 m off.
        let half = 1 << 19;
        let wide = path(&[[-half, 0], [half, 0]]);
        assert!(!wide.comes_within(at(0, 1), num(1)));
        assert!(wide.comes_within(at(0, 1), num(1) + e));
    }
}
