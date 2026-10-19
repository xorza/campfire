use std::collections::BTreeMap;

use campfire_common::DeclaredName;
use campfire_declare::{Section, SectionReaders, SectionTable, Sections, TableRead};
use campfire_geometry::{Bounds, Metric};
use campfire_math::{Num, Vec3};
use campfire_navigation::{MapPathsData, PathData, PathEnd};
use campfire_sim::Position;
use campfire_units::{BoundsData, GroundData, MapPoint, MapPointError, MetricData};
use serde::de::{DeserializeSeed, Error};
use serde::{Deserialize, Deserializer, Serialize};

use crate::error::ModeError;
use crate::mode_data::ModeParam;
use crate::region_data::RegionData;

/// One of the mode's maps, `map/<name>/map.toml`: the units placed on it from the start and its
/// markers, the mode's own, and the sections of the modules that read the rest: its metric, its
/// bounds, which every map gives, how the sim reads its ground, its grids and its paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapData {
    /// The units that stand on the map from the start.
    pub units: Vec<PlacedUnitData>,
    pub markers: Vec<MarkerData>,
    /// Its bounds among them, which its read checks.
    sections: Sections,
}

/// What reads a map's table, as serde's seed.
#[derive(Debug, Clone, Copy)]
pub struct MapSeed<'r>(&'r SectionReaders);

impl<'de> DeserializeSeed<'de> for MapSeed<'_> {
    type Value = MapData;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<MapData, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(default)]
            units: Vec<PlacedUnitData>,
            #[serde(default)]
            markers: Vec<MarkerData>,
        }
        let table = self.0.table(SectionTable::Map);
        let TableRead { sections, rest, .. } = table.deserialize(deserializer)?;
        if !sections.holds::<BoundsData>() {
            return Err(D::Error::missing_field(BoundsData::KEY));
        }
        let Fields { units, markers } = TableRead::own(rest)?;
        Ok(MapData {
            units,
            markers,
            sections,
        })
    }
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
    /// What reads a map's table, its sections as `readers` register them; an error for a key
    /// neither the map nor a section reads, a value that is no value of its type, or a map without
    /// its bounds.
    pub const fn seed(readers: &SectionReaders) -> MapSeed<'_> {
        MapSeed(readers)
    }

    /// Its section `S`, if it gives it.
    pub fn section<S: Section>(&self) -> Option<&S> {
        debug_assert_eq!(S::TABLE, SectionTable::Map, "{}", S::KEY);
        self.sections.get()
    }

    /// Its sections, its bounds among them.
    pub const fn sections(&self) -> &Sections {
        &self.sections
    }

    /// How it measures distance: on the ground plane when it names no metric.
    pub fn metric(&self) -> Metric {
        self.section::<MetricData>()
            .map_or_else(Metric::default, |metric| metric.0)
    }

    pub fn bounds(&self) -> Bounds {
        self.section::<BoundsData>()
            .expect("a map gives its bounds, as its read checks")
            .0
    }

    /// How the sim reads its height grid, `map/<name>/heights.bin`, which a map of this section
    /// has.
    pub fn ground_data(&self) -> Option<GroundData> {
        self.section::<GroundData>().copied()
    }

    /// Its paths, in order.
    pub fn paths(&self) -> &[PathData] {
        self.section::<MapPathsData>()
            .map_or(&[], |paths| paths.0.as_slice())
    }

    /// The place of `point`; an error unless it fits the map's metric and lies within its
    /// bounds.
    pub(crate) fn pos(&self, point: &MapPoint) -> Result<Vec3, ModeError> {
        point
            .place(self.metric(), self.bounds())
            .map_err(|error| match error {
                MapPointError::Shape => ModeError::PointShape,
                MapPointError::OutOfBounds => ModeError::OutOfBounds,
            })
    }

    /// Where a placed unit at `point` stands: on a planar map `height` above the ground, as a
    /// planar map measures on the ground plane and keeps a position's height for its client; an
    /// error for a height on a spatial map, whose points hold their own, and for one past the
    /// world's bound.
    pub fn placed_at(&self, point: &MapPoint, height: Num) -> Result<Vec3, ModeError> {
        let pos = self.pos(point)?;
        match self.metric() {
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
}

/// Whether `value` is the value a map's read takes for a field left out.
fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::collections::BTreeMap;

    use campfire_common::DeclaredName;
    use campfire_declare::{Section, SectionTable, Sections};
    use campfire_geometry::Bounds;
    use campfire_math::Num;
    use campfire_units::{BoundsData, MapPoint};

    use crate::map_data::{MapData, MarkerData, PlacedUnitData};

    fn name(text: &str) -> DeclaredName {
        DeclaredName::new(text).unwrap()
    }

    impl MarkerData {
        /// A marker `marker` with `tags` at `point`, of no team, region or param.
        pub fn tagged(marker: &str, tags: &[&str], point: MapPoint) -> Self {
            Self {
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
        pub fn new(unit_type: &str, team: &str, point: MapPoint) -> Self {
            Self {
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
        pub fn planar(bounds: Bounds) -> Self {
            Self {
                units: Vec::new(),
                markers: Vec::new(),
                sections: Sections::default().with(BoundsData(bounds)),
            }
        }

        /// It with `section` too, in place of one of its type.
        #[must_use]
        pub fn with<S: Section>(self, section: S) -> Self {
            debug_assert_eq!(S::TABLE, SectionTable::Map, "{}", S::KEY);
            Self {
                sections: self.sections.with(section),
                ..self
            }
        }

        /// Its section `S`, which a test changes in place, if it gives it.
        pub fn section_mut<S: Section>(&mut self) -> Option<&mut S> {
            debug_assert_eq!(S::TABLE, SectionTable::Map, "{}", S::KEY);
            self.sections.get_mut()
        }
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::{Toml, TomlError, TomlTable};
    use campfire_navigation::{MapNavigationData, PathWalk, WallData};
    use campfire_vision::{BrushData, GridData};

    use super::*;

    /// The readers of every section a map of a test holds.
    fn readers() -> SectionReaders {
        let mut readers = SectionReaders::default();
        readers.add::<MetricData>(None);
        readers.add::<BoundsData>(None);
        readers.add::<GroundData>(None);
        readers.add::<GridData>(None);
        readers.add::<MapNavigationData>(None);
        readers.add::<MapPathsData>(None);
        readers
    }

    fn read(text: &str) -> Result<MapData, TomlError> {
        Toml::parse::<TomlTable>(text)
            .unwrap()
            .read_with(MapData::seed(&readers()))
    }

    #[test]
    fn a_map_reads_its_own_fields_and_each_section_apart() {
        // Every section: a spatial metric, a grid with a brush, navigation with a wall on a
        // layer and one on the first, a path, a unit on it, turned and raised, walking it from
        // its end, a unit of no path, and a marker of a region, a team and two params.
        let text = r#"
            metric = "spatial"
            bounds = { min = [-10, -10], max = [10, 10] }
            grid = { cell = 2, brush = [{ points = [[0, 0, 0], [2, 0, 0], [0, 0, 2]] }] }
            navigation = { cell = "0.5", walls = [{ layer = "air", points = [[1, 0, 1], [3, 0, 1], [1, 0, 3]] }, { points = [[4, 0, 4], [5, 0, 4], [4, 0, 5]] }] }
            paths = [
                { name = "lane", points = [[0, 0, 0], [5, 0, 5]] },
                { name = "beat", points = [[1, 0, 1]], walk = "back_and_forth" },
            ]
            units = [
                { unit_type = "creep", team = "red", pos = [1, 2, 3], angle = "22.5", height = 4, path = "lane", from = "end" },
                { unit_type = "tower", team = "blue", pos = [-1, 0, -1] },
            ]
            markers = [{ name = "camp", tags = ["spawn"], region = { min = [0, 0, 0], max = [2, 2, 2] }, team = "red", params = { count = 3, kind = "wolf" } }]
        "#;
        let map = read(text).unwrap();
        let int = Num::int;
        let space = |x, y, z| MapPoint::Space([int(x), int(y), int(z)]);
        assert_eq!(map.metric(), Metric::Spatial);
        assert_eq!(
            map.bounds(),
            Bounds::new([int(-10), int(-10)], [int(10), int(10)]).unwrap()
        );
        assert_eq!(map.ground_data(), None);
        assert_eq!(
            map.section::<GridData>(),
            Some(&GridData {
                cell: int(2),
                brush: vec![BrushData {
                    points: vec![space(0, 0, 0), space(2, 0, 0), space(0, 0, 2)],
                }],
            })
        );
        let navigation = map.section::<MapNavigationData>().unwrap();
        assert_eq!(navigation.cell, Num::ONE / int(2));
        assert_eq!(
            navigation.walls[1],
            WallData {
                layer: None,
                points: vec![space(4, 0, 4), space(5, 0, 4), space(4, 0, 5)],
            }
        );
        assert_eq!(
            map.paths(),
            [
                PathData {
                    name: DeclaredName::new("lane").unwrap(),
                    points: vec![space(0, 0, 0), space(5, 0, 5)],
                    walk: PathWalk::Once,
                },
                PathData {
                    name: DeclaredName::new("beat").unwrap(),
                    points: vec![space(1, 0, 1)],
                    walk: PathWalk::BackAndForth,
                }
            ]
        );
        assert_eq!((map.units.len(), map.markers.len()), (2, 1));
        // The map's own fields and its sections are all it reads.
        let stray = read("bounds = { min = [0, 0], max = [1, 1] }\nroads = []").unwrap_err();
        assert!(
            stray.message().starts_with("unknown field `roads`"),
            "{stray}"
        );
        // Every map gives its bounds.
        assert_eq!(
            read("units = []").unwrap_err().message(),
            "missing field `bounds`"
        );
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
    fn a_placed_unit_stands_its_height_above_the_ground_or_on_it() {
        let map = read(
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
