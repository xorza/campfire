use std::collections::BTreeMap;

use campfire_common::DeclaredName;
use campfire_declarations::{
    PatchError, Patched, Section, SectionReaders, SectionTable, Sections, TableRead,
};
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
        let table = self.0.table(SectionTable::Map);
        let TableRead { sections, rest, .. } = table.deserialize(deserializer)?;
        if !sections.holds::<BoundsData>() {
            return Err(D::Error::missing_field(BoundsData::KEY));
        }
        let data: MapData = TableRead::own(rest)?;
        Ok(MapData { sections, ..data })
    }
}

/// A map's own keys as a tool writes them: each of `MapData`'s, but its sections, optional
/// (03-game-scripting.md, A section has a partial form).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub units: Option<Vec<PlacedUnitData>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markers: Option<Vec<MarkerData>>,
}

impl Patched for MapData {
    type Patch = MapPatch;

    fn from_patch(patch: MapPatch) -> Result<Self, PatchError> {
        Ok(Self {
            units: patch.units.unwrap_or_default(),
            markers: patch.markers.unwrap_or_default(),
            sections: Sections::default(),
        })
    }
}

impl<'de> Deserialize<'de> for MapData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::read_patch(deserializer)
    }
}

/// A unit that stands on the map from the start, of a team; turned by `angle` degrees if it has
/// a box body; `height` meters above the ground beneath `position`; on a path if it guards one, and
/// walking it from the end `from` names if it walks one; of the place among its type's variations
/// `variation` names, if it names one, else of one its spawn draws
/// (04-capabilities/00-overview.md, A placement may give its variation).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacedUnitData {
    pub unit_type: DeclaredName,
    pub team: DeclaredName,
    pub position: MapPoint,
    #[serde(default, skip_serializing_if = "is_default")]
    pub angle: Num,
    #[serde(default, skip_serializing_if = "is_default")]
    pub height: Num,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<DeclaredName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<PathEnd>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variation: Option<u8>,
}

/// A named place of the map, which scripts read by its tags: a point, or a box of `region`, with
/// a team and params when it names them, such as a team's spawn or a camp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkerData {
    pub name: DeclaredName,
    pub tags: Vec<DeclaredName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<MapPoint>,
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
    pub(crate) fn position(&self, point: &MapPoint) -> Result<Vec3, ModeError> {
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
        let position = self.position(point)?;
        match self.metric() {
            _ if height == Num::ZERO => Ok(position),
            Metric::Spatial => Err(ModeError::SpatialHeight),
            Metric::Planar => {
                let placed = Vec3::new(position.x, height, position.z);
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
    use campfire_declarations::{Section, SectionTable, Sections};
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
                position: Some(point),
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
                position: point,
                angle: Num::ZERO,
                height: Num::ZERO,
                path: None,
                from: None,
                variation: None,
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
mod tests;
