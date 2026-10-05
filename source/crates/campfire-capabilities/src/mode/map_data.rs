use std::collections::BTreeMap;

use campfire_math::{Num, Vec3};
use campfire_sim::{IdAllocator, Position};
use serde::Deserialize;

use crate::mode::error::ModeError;
use crate::mode::mode_data::ModeParam;
use crate::navigation::body_index::{BodyIndex, IndexedBody};
use crate::navigation::error::MapProblem;
use crate::navigation::navigation_rules::NavigationRules;
use crate::navigation::path_walker::PathEnd;
use crate::navigation::pathing_grid::PathingGrid;
use crate::navigation::route_planner::Walkable;
use crate::navigation::segment::Segment;
use crate::navigation::terrain::Terrain;
use crate::navigation::walker::Walker;
use crate::navigation::wall::Wall;
use crate::units::body::Body;
use crate::units::layer::Layer;
use crate::values::bounds::Bounds;
use crate::values::declared_name::DeclaredName;
use crate::values::grid::Grid;
use crate::values::metric::Metric;
use crate::values::polygon::Polygon;
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
    /// The cells units plan routes over, over the bounds, and the walls that block them; a mode
    /// that declares `navigation` has one.
    pub navigation: Option<MapNavigationData>,
    #[serde(default)]
    pub paths: Vec<PathData>,
    /// The units that stand on the map from the start.
    #[serde(default)]
    pub units: Vec<PlacedUnitData>,
    #[serde(default)]
    pub markers: Vec<MarkerData>,
}

/// A map's `[grid]`: the size of the square cells vision reveals, in meters, and its brush.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GridData {
    pub cell: Scalar,
    #[serde(default)]
    pub brush: Vec<BrushData>,
}

/// A brush, `[[grid.brush]]`: the polygon of `points`, whose cells only a unit in it reveals.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrushData {
    pub points: Vec<MapPoint>,
}

/// A map's `[navigation]`: the size of the square cells routes are planned on, in meters, and its
/// walls.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapNavigationData {
    pub cell: Scalar,
    #[serde(default)]
    pub walls: Vec<WallData>,
}

/// A wall, `[[navigation.walls]]`: on `layer`, the mode's first when it names none, the polygon of
/// `points`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WallData {
    pub layer: Option<DeclaredName>,
    pub points: Vec<MapPoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathData {
    pub name: DeclaredName,
    pub points: Vec<MapPoint>,
}

/// A unit that stands on the map from the start, of a team; on a path if it guards one, and
/// walking it from the end `from` names if it walks one.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacedUnitData {
    pub unit_type: DeclaredName,
    pub team: DeclaredName,
    pub pos: MapPoint,
    pub path: Option<DeclaredName>,
    pub from: Option<PathEnd>,
}

/// A named place of the map, which scripts read by its tags: a point, or a box of `region`, with
/// a team and params when it names them, such as a team's spawn or a camp.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkerData {
    pub name: DeclaredName,
    pub tags: Vec<DeclaredName>,
    pub pos: Option<MapPoint>,
    pub region: Option<RegionData>,
    pub team: Option<DeclaredName>,
    #[serde(default)]
    pub params: BTreeMap<DeclaredName, ModeParam>,
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
    /// Checks that the map can be walked by every kind of unit that walks, of `walkers`, for the
    /// widest of each layer, among the map's placed units that cannot walk, whose bodies
    /// `body_of` gives by unit type, and none for a type that walks: every marker's point and
    /// waypoint is a place that walker may stand, and every waypoint is in a reachable set of the
    /// one before it, by the regions a match plans its routes with. A narrower walker of the
    /// layer has every cell the widest has open. A map with no `[navigation]` cells, or a mode
    /// with no walker, has nothing to check. The book build checked its points.
    pub fn check_walkable(
        &self,
        walkers: &[Walker],
        rules: &NavigationRules,
        body_of: impl Fn(&str) -> Option<Body>,
    ) -> Result<(), MapProblem> {
        let Some(cells) = self.pathing().expect("the book build checked the map") else {
            return Ok(());
        };
        let walls = self.walls(rules).expect("the book build checked the map");
        let terrain = Terrain::new(&cells, &walls);
        debug_assert!(walkers.is_sorted(), "walkers by layer, then radius");
        let widest = walkers
            .chunk_by(|a, b| a.layer == b.layer)
            .map(|layer| *layer.last().expect("a chunk is never empty"));
        let point = |point: &MapPoint| point.position().expect("the book build checked the map");
        let mut ids = IdAllocator::default();
        let structures: Vec<IndexedBody> = self
            .units
            .iter()
            .filter_map(|unit| {
                let body = body_of(unit.unit_type.as_str())?;
                Some(IndexedBody {
                    id: ids.allocate(),
                    at: point(&unit.pos),
                    radius: body.radius(),
                    layer: body.layer(),
                })
            })
            .collect();
        let widest_radius = walkers.iter().map(|walker| walker.radius).max();
        let mut statics = BodyIndex::new(widest_radius.unwrap_or(Num::ZERO));
        statics.update(&structures);
        let mut grid = PathingGrid::new(cells, widest.clone().collect(), &terrain);
        grid.update(&statics);
        for walker in widest {
            let clearance = grid.clearance(walker);
            let walkable = Walkable {
                clearance,
                statics: &statics,
                short: None,
            };
            let stands = |at: Position| !walkable.blocks(Segment::new(at, at));
            for marker in &self.markers {
                if marker.pos.is_some_and(|pos| !stands(point(&pos))) {
                    let marker = marker.name.clone();
                    return Err(MapProblem::MarkerBlocked { marker });
                }
            }
            let reach = |at: Position| clearance.regions().reach(cells.nearest_cell(at));
            for path in &self.paths {
                if let Some(waypoint) = path.points.iter().position(|ground| !stands(point(ground)))
                {
                    let path = path.name.clone();
                    return Err(MapProblem::WaypointBlocked { path, waypoint });
                }
                let closed = path
                    .points
                    .windows(2)
                    .position(|pair| !reach(point(&pair[0])).meets(reach(point(&pair[1]))));
                if let Some(before) = closed {
                    let (path, waypoint) = (path.name.clone(), before + 1);
                    return Err(MapProblem::WaypointUnreachable { path, waypoint });
                }
            }
        }
        Ok(())
    }

    /// The vision grid over the bounds, if the map has one; an error unless its cell is positive
    /// and at most the world's bound, and it has at most 2²² cells.
    pub fn grid(&self) -> Result<Option<Grid>, ModeError> {
        self.grid_of(self.grid.as_ref().map(|grid| grid.cell))
    }

    /// The areas of the map's brush, in its order; an error for a point that does not fit the
    /// map's metric or lies outside its bounds, and points that make no simple polygon.
    pub fn brush(&self) -> Result<Vec<Polygon>, ModeError> {
        let Some(grid) = &self.grid else {
            return Ok(Vec::new());
        };
        let mut areas = Vec::with_capacity(grid.brush.len());
        for (brush, data) in grid.brush.iter().enumerate() {
            let points = self.ground(&data.points)?;
            areas
                .push(Polygon::new(points).map_err(|problem| ModeError::Brush { brush, problem })?);
        }
        Ok(areas)
    }

    /// `points` on the ground plane; an error as `point` gives.
    fn ground(&self, points: &[MapPoint]) -> Result<Vec<[Num; 2]>, ModeError> {
        points
            .iter()
            .map(|point| {
                let at = self.point(point)?.get();
                Ok([at.x, at.z])
            })
            .collect()
    }

    /// The pathing grid's cells over the bounds, if the map has them; an error as for `grid`.
    pub fn pathing(&self) -> Result<Option<Grid>, ModeError> {
        self.grid_of(self.navigation.as_ref().map(|navigation| navigation.cell))
    }

    /// The map's walls, each on the layer of `rules` it names; an error for a layer the mode does
    /// not declare, a point that does not fit the map's metric or lies outside its bounds, and
    /// points that make no simple polygon.
    pub(crate) fn walls(&self, rules: &NavigationRules) -> Result<Vec<Wall>, ModeError> {
        let Some(navigation) = &self.navigation else {
            return Ok(Vec::new());
        };
        let mut walls = Vec::with_capacity(navigation.walls.len());
        for (wall, data) in navigation.walls.iter().enumerate() {
            let layer = match &data.layer {
                Some(name) => rules
                    .layer_named(name)
                    .ok_or_else(|| ModeError::UnknownLayer(name.clone()))?,
                None => Layer::FIRST,
            };
            let area = Polygon::new(self.ground(&data.points)?)
                .map_err(|problem| ModeError::Wall { wall, problem })?;
            walls.push(Wall { layer, area });
        }
        Ok(walls)
    }

    /// `point` as a position; an error unless it fits the map's metric and lies within its
    /// bounds.
    pub(crate) fn point(&self, point: &MapPoint) -> Result<Position, ModeError> {
        match point.position() {
            _ if !point.fits(self.metric) => Err(ModeError::PointShape),
            Some(pos) if self.bounds.contains(pos) => Ok(pos),
            _ => Err(ModeError::OutOfBounds),
        }
    }

    fn grid_of(&self, cell: Option<Scalar>) -> Result<Option<Grid>, ModeError> {
        let Some(cell) = cell else {
            return Ok(None);
        };
        let grid = cell.to_num().and_then(|cell| Grid::new(cell, self.bounds));
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
pub(crate) mod internals {
    use std::collections::BTreeMap;

    use crate::mode::map_data::{MapData, MapPoint, MarkerData, PlacedUnitData};
    use crate::values::bounds::Bounds;
    use crate::values::declared_name::DeclaredName;
    use crate::values::metric::Metric;
    use crate::values::scalar::Scalar;

    fn name(text: &str) -> DeclaredName {
        DeclaredName::new(text).unwrap()
    }

    impl MapPoint {
        pub(crate) const fn ground(x: i64, z: i64) -> MapPoint {
            MapPoint::Ground([Scalar::Int(x), Scalar::Int(z)])
        }
    }

    impl MarkerData {
        /// A marker `marker` with `tags` at `pos`, of no team, region or param.
        pub(crate) fn tagged(marker: &str, tags: &[&str], pos: MapPoint) -> MarkerData {
            MarkerData {
                name: name(marker),
                tags: tags.iter().map(|&tag| name(tag)).collect(),
                pos: Some(pos),
                region: None,
                team: None,
                params: BTreeMap::new(),
                events: false,
            }
        }
    }

    impl PlacedUnitData {
        /// A unit of `unit_type` and `team` at `pos`, on no path.
        pub(crate) fn new(unit_type: &str, team: &str, pos: MapPoint) -> PlacedUnitData {
            PlacedUnitData {
                unit_type: name(unit_type),
                team: name(team),
                pos,
                path: None,
                from: None,
            }
        }
    }

    impl MapData {
        /// A planar map of `bounds` and nothing more: no grid, path, unit or marker.
        pub(crate) const fn planar(bounds: Bounds) -> MapData {
            MapData {
                metric: Metric::Planar,
                bounds,
                grid: None,
                navigation: None,
                paths: Vec::new(),
                units: Vec::new(),
                markers: Vec::new(),
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::values::polygon::error::PolygonError;

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

    #[test]
    fn a_brush_loads_only_as_a_simple_polygon_within_the_bounds() {
        let bounds = Bounds::new([Num::ZERO; 2], [Num::int(4); 2]).unwrap();
        let with = |points: &[(i64, i64)]| MapData {
            grid: Some(GridData {
                cell: Scalar::Int(1),
                brush: vec![BrushData {
                    points: points
                        .iter()
                        .map(|&(x, z)| MapPoint::ground(x, z))
                        .collect(),
                }],
            }),
            ..MapData::planar(bounds)
        };
        let fault = |problem| Err(ModeError::Brush { brush: 0, problem });
        assert_eq!(
            with(&[(0, 0), (4, 0), (0, 5)]).brush(),
            Err(ModeError::OutOfBounds)
        );
        assert_eq!(
            with(&[(0, 0), (4, 0)]).brush(),
            fault(PolygonError::TooFewPoints)
        );
        let crossed = PolygonError::EdgesMeet {
            first: 0,
            second: 2,
        };
        assert_eq!(
            with(&[(0, 0), (4, 4), (4, 0), (0, 4)]).brush(),
            fault(crossed)
        );
        let triangle = with(&[(0, 0), (4, 0), (0, 4)]).brush().unwrap();
        assert_eq!(triangle.len(), 1);
        // A map with no vision grid has no brush.
        assert_eq!(MapData::planar(bounds).brush(), Ok(Vec::new()));
    }
}
