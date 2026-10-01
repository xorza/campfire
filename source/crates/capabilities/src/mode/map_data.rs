use std::collections::BTreeMap;

use campfire_math::Vec3;
use campfire_sim::Position;
use serde::Deserialize;

use crate::mode::error::ModeError;
use crate::mode::mode_data::ModeParam;
use crate::navigation::path_walker::PathEnd;
use crate::values::bounds::Bounds;
use crate::values::grid::Grid;
use crate::values::metric::Metric;
use crate::values::scalar::Scalar;

/// The mode's `map/map.toml`: its metric, its bounds, its grids, its paths, the units placed on it
/// from the start, and its markers.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapData {
    #[serde(default)]
    pub metric: Metric,
    pub bounds: Bounds,
    /// The cells vision reveals, over the bounds; a mode that declares `vision` has one.
    pub grid: Option<GridData>,
    /// The cells units plan routes over, over the bounds; a mode that declares `navigation` has
    /// one.
    pub navigation: Option<GridData>,
    #[serde(default)]
    pub paths: Vec<PathData>,
    /// The units that stand on the map from the start.
    #[serde(default)]
    pub units: Vec<PlacedUnitData>,
    #[serde(default)]
    pub markers: Vec<MarkerData>,
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
    pub points: Vec<MapPoint>,
}

/// A unit that stands on the map from the start, of a team; on a path if it guards one, and
/// walking it from the end `from` names if it walks one.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacedUnitData {
    pub unit_type: String,
    pub team: String,
    pub pos: MapPoint,
    pub path: Option<String>,
    pub from: Option<PathEnd>,
}

/// A named place of the map, which scripts read by its tags: a point, or a box of `region`, with
/// a team and params when it names them, such as a team's spawn or a camp.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkerData {
    pub name: String,
    pub tags: Vec<String>,
    pub pos: Option<MapPoint>,
    pub region: Option<RegionData>,
    pub team: Option<String>,
    #[serde(default)]
    pub params: BTreeMap<String, ModeParam>,
    /// Whether the mode hears units enter and leave its region.
    #[serde(default)]
    pub events: bool,
}

/// A box of the map, from `min` to `max`, `min` below `max` on every axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionData {
    pub min: MapPoint,
    pub max: MapPoint,
}

/// A point of the map, in meters: `[x, z]` on the ground plane of a planar map, `[x, y, z]` on a
/// spatial one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum MapPoint {
    Ground([Scalar; 2]),
    Space([Scalar; 3]),
}

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

impl MapPoint {
    /// The point as a position, a ground point at height 0; `None` beyond the world's bound.
    pub fn position(self) -> Option<Position> {
        let [x, y, z] = match self {
            MapPoint::Ground([x, z]) => [x, Scalar::Int(0), z],
            MapPoint::Space(point) => point,
        }
        .map(Scalar::to_num);
        Position::new(Vec3::new(x?, y?, z?))
    }

    /// Whether it has the shape of a point of a map of `metric`.
    pub const fn fits(self, metric: Metric) -> bool {
        matches!(
            (self, metric),
            (MapPoint::Ground(_), Metric::Planar) | (MapPoint::Space(_), Metric::Spatial)
        )
    }
}

impl RegionData {
    /// Whether its points fit `metric` and lie within `bounds`, `min` below `max` on every axis
    /// of the metric.
    pub fn holds(self, metric: Metric, bounds: Bounds) -> bool {
        let (Some(min), Some(max)) = (self.min.position(), self.max.position()) else {
            return false;
        };
        let (low, high) = (min.get(), max.get());
        let below =
            low.x < high.x && low.z < high.z && (metric == Metric::Planar || low.y < high.y);
        let fits = self.min.fits(metric) && self.max.fits(metric);
        fits && below && bounds.contains(min) && bounds.contains(max)
    }
}

#[cfg(test)]
mod tests {
    use campfire_math::Num;

    use super::*;

    #[test]
    fn a_point_beyond_the_world_bound_has_no_position() {
        let int = Scalar::Int;
        let at = |x: i64, y: i64, z: i64| {
            let num = |value| Num::from_int(value).unwrap();
            Position::new(Vec3::new(num(x), num(y), num(z)))
        };
        // A ground point stands at height 0; 2⁴⁰ m is past what a `Num` holds, on any axis.
        let beyond = 1 << 40;
        for (point, position) in [
            (MapPoint::Ground([int(1), int(2)]), at(1, 0, 2)),
            (MapPoint::Space([int(1), int(3), int(2)]), at(1, 3, 2)),
            (MapPoint::Space([int(1), int(beyond), int(2)]), None),
            (MapPoint::Ground([int(beyond), int(2)]), None),
        ] {
            assert_eq!(point.position(), position, "{point:?}");
        }
    }
}
