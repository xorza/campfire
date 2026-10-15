use std::collections::BTreeMap;

use campfire_geometry::{Bounds, Grid, Metric, Polygon};
use campfire_math::{Num, Vec3};
use campfire_navigation::{PathEnd, Wall};
use campfire_sim::Position;
use campfire_units::Layer;
use campfire_values::{DeclaredName, Names};
use serde::{Deserialize, Serialize};

use crate::error::ModeError;
use crate::map_point::MapPoint;
use crate::mode_data::ModeParam;
use crate::region_data::RegionData;

/// One of the mode's maps, `map/<name>/map.toml`: its metric, its bounds, its grids, its paths,
/// the units placed on it from the start, and its markers. It writes as it reads, but a field at
/// the value its read takes when the field is left out, which it leaves out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapData {
    #[serde(default, skip_serializing_if = "is_default")]
    pub metric: Metric,
    pub bounds: Bounds,
    /// The cells vision reveals, over the bounds; a mode that declares `vision` has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grid: Option<GridData>,
    /// The cells units plan routes over, over the bounds, and the walls that block them; a mode
    /// that declares `navigation` has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub navigation: Option<MapNavigationData>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub paths: Vec<PathData>,
    /// The units that stand on the map from the start.
    #[serde(default, skip_serializing_if = "is_default")]
    pub units: Vec<PlacedUnitData>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub markers: Vec<MarkerData>,
}

/// A map's `[grid]`: the size of the square cells vision reveals, in meters, and its brush.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GridData {
    pub cell: Num,
    #[serde(default, skip_serializing_if = "is_default")]
    pub brush: Vec<BrushData>,
}

/// A brush, `[[grid.brush]]`: the polygon of `points`, whose cells only a unit in it reveals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrushData {
    pub points: Vec<MapPoint>,
}

/// A map's `[navigation]`: the size of the square cells routes are planned on, in meters, and its
/// walls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapNavigationData {
    pub cell: Num,
    #[serde(default, skip_serializing_if = "is_default")]
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

/// A path of the map: its name, and its waypoints in the order a unit walks them from its start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathData {
    pub name: DeclaredName,
    pub points: Vec<MapPoint>,
}

/// A unit that stands on the map from the start, of a team; turned by `angle` degrees if it has
/// a box body; `height` meters above the ground beneath `pos`; on a path if it guards one, and
/// walking it from the end `from` names if it walks one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacedUnitData {
    pub unit_type: DeclaredName,
    pub team: DeclaredName,
    pub pos: MapPoint,
    #[serde(default, skip_serializing_if = "is_default")]
    pub angle: Num,
    #[serde(default, skip_serializing_if = "is_default")]
    pub height: Num,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<DeclaredName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<PathEnd>,
}

/// A named place of the map, which scripts read by its tags: a point, or a box of `region`, with
/// a team and params when it names them, such as a team's spawn or a camp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkerData {
    pub name: DeclaredName,
    pub tags: Vec<DeclaredName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos: Option<MapPoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<RegionData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team: Option<DeclaredName>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub params: BTreeMap<DeclaredName, ModeParam>,
}

impl MapData {
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
                let at = self.pos(point)?;
                Ok([at.x, at.z])
            })
            .collect()
    }

    /// The pathing grid's cells over the bounds, if the map has them; an error as for `grid`.
    pub fn pathing(&self) -> Result<Option<Grid>, ModeError> {
        self.grid_of(self.navigation.as_ref().map(|navigation| navigation.cell))
    }

    /// The map's walls, each on the layer of `layers` it names; an error for a layer the mode does
    /// not declare, a point that does not fit the map's metric or lies outside its bounds, and
    /// points that make no simple polygon.
    pub fn walls(&self, layers: &Names<Layer>) -> Result<Vec<Wall>, ModeError> {
        let Some(navigation) = &self.navigation else {
            return Ok(Vec::new());
        };
        let mut walls = Vec::with_capacity(navigation.walls.len());
        for (wall, data) in navigation.walls.iter().enumerate() {
            let layer = match &data.layer {
                Some(name) => layers
                    .named(name.as_str())
                    .ok_or_else(|| ModeError::UnknownLayer(name.clone()))?,
                None => Layer::FIRST,
            };
            let area = Polygon::new(self.ground(&data.points)?)
                .map_err(|problem| ModeError::Wall { wall, problem })?;
            walls.push(Wall { layer, area });
        }
        Ok(walls)
    }

    /// The place of `point`; an error unless it fits the map's metric and lies within its
    /// bounds.
    pub(crate) fn pos(&self, point: &MapPoint) -> Result<Vec3, ModeError> {
        match point.pos() {
            _ if !point.fits(self.metric) => Err(ModeError::PointShape),
            Some(pos) if self.bounds.contains(pos) => Ok(pos),
            _ => Err(ModeError::OutOfBounds),
        }
    }

    /// Where a placed unit at `point` stands: on a planar map `height` above the ground, as a
    /// planar map measures on the ground plane and keeps a position's height for its client; an
    /// error for a height on a spatial map, whose points hold their own, and for one past the
    /// world's bound.
    pub fn placed_at(&self, point: &MapPoint, height: Num) -> Result<Vec3, ModeError> {
        let pos = self.pos(point)?;
        match self.metric {
            _ if height == Num::ZERO => Ok(pos),
            Metric::Spatial => Err(ModeError::SpatialHeight),
            Metric::Planar => {
                let placed = Vec3::new(pos.x, height, pos.z);
                Position::from(placed)
                    .within_bound()
                    .then_some(placed)
                    .ok_or(ModeError::OutOfBounds)
            }
        }
    }

    fn grid_of(&self, cell: Option<Num>) -> Result<Option<Grid>, ModeError> {
        let Some(cell) = cell else {
            return Ok(None);
        };
        let grid = Grid::new(cell, self.bounds);
        grid.map(Some).ok_or(ModeError::Grid)
    }
}

/// Whether `value` is the value a map's read takes for a field left out.
fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::collections::BTreeMap;

    use campfire_geometry::{Bounds, Metric};
    use campfire_math::Num;
    use campfire_values::DeclaredName;

    use crate::map_data::{MapData, MarkerData, PlacedUnitData};
    use crate::map_point::MapPoint;

    fn name(text: &str) -> DeclaredName {
        DeclaredName::new(text).unwrap()
    }

    impl MapPoint {
        pub const fn ground(x: i64, z: i64) -> MapPoint {
            MapPoint::Ground([Num::int(x), Num::int(z)])
        }
    }

    impl MarkerData {
        /// A marker `marker` with `tags` at `point`, of no team, region or param.
        pub fn tagged(marker: &str, tags: &[&str], point: MapPoint) -> MarkerData {
            MarkerData {
                name: name(marker),
                tags: tags.iter().map(|&tag| name(tag)).collect(),
                pos: Some(point),
                region: None,
                team: None,
                params: BTreeMap::new(),
            }
        }
    }

    impl PlacedUnitData {
        /// A unit of `unit_type` and `team` at `point`, on no path.
        pub fn new(unit_type: &str, team: &str, point: MapPoint) -> PlacedUnitData {
            PlacedUnitData {
                unit_type: name(unit_type),
                team: name(team),
                pos: point,
                angle: Num::ZERO,
                height: Num::ZERO,
                path: None,
                from: None,
            }
        }
    }

    impl MapData {
        /// A planar map of `bounds` and nothing more: no grid, path, unit or marker.
        pub const fn planar(bounds: Bounds) -> MapData {
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

    use campfire_codec::Toml;
    use campfire_geometry::PolygonError;

    use super::*;

    #[test]
    fn a_map_writes_what_it_reads_and_leaves_out_each_default() {
        // Every section: a spatial metric, a grid with a brush, navigation with a wall on a
        // layer and one on the first, a path, a unit on it, turned and raised, walking it from
        // its end, a unit of no path, and a marker of a region, a team and two params.
        let text = r#"
metric = "spatial"
bounds = { min = [-10, -10], max = [10, 10] }
grid = { cell = 2, brush = [{ points = [[0, 0, 0], [2, 0, 0], [0, 0, 2]] }] }
navigation = { cell = "0.5", walls = [{ layer = "air", points = [[1, 0, 1], [3, 0, 1], [1, 0, 3]] }, { points = [[4, 0, 4], [5, 0, 4], [4, 0, 5]] }] }
paths = [{ name = "lane", points = [[0, 0, 0], [5, 0, 5]] }]
units = [
    { unit_type = "creep", team = "red", pos = [1, 2, 3], angle = "22.5", height = 4, path = "lane", from = "end" },
    { unit_type = "tower", team = "blue", pos = [-1, 0, -1] },
]
markers = [{ name = "camp", tags = ["spawn"], region = { min = [0, 0, 0], max = [2, 2, 2] }, team = "red", params = { count = 3, kind = "wolf" } }]
"#;
        let map: MapData = Toml::parse(text).unwrap();
        assert_eq!(Toml::parse::<MapData>(&Toml::write(&map)).unwrap(), map);
        // A planar map of its bounds alone writes them alone: the metric, the empty lists and
        // the absent sections are left out, as the read takes them for each left out.
        let bounds = Bounds::new([Num::int(-1), Num::int(-2)], [Num::int(3), Num::int(4)]).unwrap();
        let bare = MapData::planar(bounds);
        let written = Toml::write(&bare);
        assert_eq!(
            written,
            "[bounds]\nmin = [\"-1\", \"-2\"]\nmax = [\"3\", \"4\"]\n"
        );
        assert_eq!(Toml::parse::<MapData>(&written).unwrap(), bare);
        // A unit at no angle and height, and a marker of no region, team or param, write their
        // other fields alone.
        let unit = PlacedUnitData::new("creep", "red", MapPoint::ground(1, 2));
        assert_eq!(
            Toml::write(&unit),
            "unit_type = \"creep\"\nteam = \"red\"\npos = [\"1\", \"2\"]\n"
        );
        let marker = MarkerData::tagged("w1", &["waypoint"], MapPoint::ground(0, 0));
        assert_eq!(
            Toml::write(&marker),
            "name = \"w1\"\ntags = [\"waypoint\"]\npos = [\"0\", \"0\"]\n"
        );
    }

    #[test]
    fn a_point_beyond_the_world_bound_has_no_position() {
        let int = Num::int;
        let at = |x: i64, y: i64, z: i64| Some(Vec3::new(int(x), int(y), int(z)));
        // A ground point stands at height 0; a meter past 2²⁰ m is past the world's bound, on any
        // axis.
        let beyond = (1 << 20) + 1;
        for (point, pos) in [
            (MapPoint::Ground([int(1), int(2)]), at(1, 0, 2)),
            (MapPoint::Space([int(1), int(3), int(2)]), at(1, 3, 2)),
            (MapPoint::Space([int(1), int(beyond), int(2)]), None),
            (MapPoint::Ground([int(beyond), int(2)]), None),
        ] {
            assert_eq!(point.pos(), pos, "{point:?}");
        }
    }

    #[test]
    fn a_brush_loads_only_as_a_simple_polygon_within_the_bounds() {
        let bounds = Bounds::new([Num::ZERO; 2], [Num::int(4); 2]).unwrap();
        let with = |points: &[(i64, i64)]| MapData {
            grid: Some(GridData {
                cell: Num::ONE,
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

    #[test]
    fn a_placed_unit_stands_its_height_above_the_ground_or_on_it() {
        let map: MapData = Toml::parse(
            r#"
            bounds = { min = [0, 0], max = [4, 4] }
            [[units]]
            unit_type = "rock"
            team = "neutral"
            pos = [1, 2]
            angle = "22.5"
            height = "-4.5"
            [[units]]
            unit_type = "rock"
            team = "neutral"
            pos = [3, 2]
            "#,
        )
        .unwrap();
        let rock = |pos, angle, height| PlacedUnitData {
            angle,
            height,
            ..PlacedUnitData::new("rock", "neutral", pos)
        };
        assert_eq!(
            map.units,
            [
                rock(
                    MapPoint::ground(1, 2),
                    Num::int(45) / Num::int(2),
                    Num::int(-9) / Num::int(2)
                ),
                rock(MapPoint::ground(3, 2), Num::ZERO, Num::ZERO),
            ]
        );
    }
}
