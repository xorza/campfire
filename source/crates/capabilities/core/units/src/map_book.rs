use bevy_ecs::resource::Resource;
use campfire_declare::{Book, BookCx, BookError, BookPlace, Section, SectionTable, Sections};
use campfire_geometry::{Bounds, Metric};
use campfire_math::{Num, Vec3};
use campfire_script::MatchNames;
use campfire_values::PackageIndex;
use serde::{Deserialize, Serialize};

use crate::error::{GroundLoadError, MapPointError};
use crate::ground::Ground;
use crate::ground_data::GroundData;
use crate::height_grid::HeightGrid;
use crate::map_point::MapPoint;

/// The map's measure of distance, its bounds and its ground; the world's bounds and a flat
/// ground before a map names its own. Package data, not state.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct MapBook {
    pub metric: Metric,
    pub bounds: Bounds,
    pub ground: Ground,
}

impl Default for MapBook {
    fn default() -> Self {
        Self {
            metric: Metric::default(),
            bounds: Bounds::WORLD,
            ground: Ground::default(),
        }
    }
}

impl MapBook {
    /// The place of the map's point `point`; an error unless it fits the map's metric and lies
    /// within its bounds.
    pub fn pos(&self, point: MapPoint) -> Result<Vec3, MapPointError> {
        point.place(self.metric, self.bounds)
    }

    /// The point in space of the position `pos`: on a planar map, its ground's height at it
    /// raised by the position's height above the ground (04-capabilities/00-overview.md, The sim's
    /// ground is the map's height grid); on a spatial map, the position itself.
    pub fn point(&self, pos: Vec3) -> Vec3 {
        match self.metric {
            Metric::Planar => Vec3::new(pos.x, self.ground.height(pos.x, pos.z) + pos.y, pos.z),
            Metric::Spatial => pos,
        }
    }

    /// The position of the point in space `point`, as `point` gives it back.
    pub fn position(&self, point: Vec3) -> Vec3 {
        match self.metric {
            Metric::Planar => {
                let ground = self.ground.height(point.x, point.z);
                Vec3::new(point.x, point.y - ground, point.z)
            }
            Metric::Spatial => point,
        }
    }

    /// The highest of the ground's coarse cells the line from `a` to `b` crosses on the ground
    /// plane, as `Ground::highest_along` finds it; none on a spatial map, which has no ground.
    pub fn highest_along(&self, a: Vec3, b: Vec3) -> Option<Num> {
        match self.metric {
            Metric::Planar => self.ground.highest_along([a.x, a.z], [b.x, b.z]),
            Metric::Spatial => None,
        }
    }
}

impl MapBook {
    /// The map's measure of distance, its bounds and its ground, of its height grid, which the load
    /// gives when the map has one, as its `[ground]` reads it; flat with no grid. An error for a
    /// `[ground]` with no grid, or coarse cells that do not fit it.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let book = Self::of(cx.source().map, cx.given::<HeightGrid>());
        let at = BookPlace::MapSection(GroundData::KEY);
        book.map_err(|problem| BookError::rule(PackageIndex::MODE, at, problem))
    }

    /// The book of a map of `sections`, its bounds among them, and of its height grid `heights`,
    /// when it has one; an error as `build` gives.
    pub fn of(sections: &Sections, heights: Option<&HeightGrid>) -> Result<Self, GroundLoadError> {
        let bounds = sections.get::<BoundsData>();
        let bounds = bounds
            .expect("a map gives its bounds, as its read checks")
            .0;
        let metric = sections.get::<MetricData>();
        let ground = match (heights, sections.get::<GroundData>()) {
            (None, None) => Ground::default(),
            (None, Some(_)) => return Err(GroundLoadError::NoHeights),
            (Some(grid), data) => {
                let GroundData { split, high } = data.copied().unwrap_or_default();
                Ground::new(grid.clone(), split, high).map_err(GroundLoadError::Ground)?
            }
        };
        Ok(Self {
            metric: metric.map_or_else(Metric::default, |metric| metric.0),
            bounds,
            ground,
        })
    }
}

impl Book for MapBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &mut MatchNames) {}
}

/// A map's `metric`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MetricData(pub Metric);

impl Section for MetricData {
    const TABLE: SectionTable = SectionTable::Map;
    const KEY: &'static str = "metric";
}

/// A map's `[bounds]`, which every map gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BoundsData(pub Bounds);

impl Section for BoundsData {
    const TABLE: SectionTable = SectionTable::Map;
    const KEY: &'static str = "bounds";
}
