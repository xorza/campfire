use bevy_ecs::resource::Resource;
use campfire_declarations::{Book, BookCx, BookError, BookPlace, Section, SectionTable, Sections};
use campfire_geometry::{Bounds, Metric};
use campfire_math::{Num, Vec3};
use campfire_names::MatchNames;
use campfire_script::{ApiVersion, Status};
use campfire_values::PackageIndex;
use serde::{Deserialize, Serialize};

use crate::error::{GroundLoadError, MapPointError};
use crate::ground::Ground;
use crate::ground_data::GroundData;
use crate::height_grid::HeightGrid;
use crate::map_point::MapPoint;
use crate::map_space::MapSpace;

/// The map's space, its ground among it on a planar map, and its bounds; the world's bounds and a
/// flat ground before a map names its own. Package data, not state.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
#[component(immutable)]
pub struct MapBook {
    pub space: MapSpace,
    pub bounds: Bounds,
}

impl Default for MapBook {
    fn default() -> Self {
        Self {
            space: MapSpace::default(),
            bounds: Bounds::WORLD,
        }
    }
}

impl MapBook {
    /// The map's measure of distance.
    pub const fn metric(&self) -> Metric {
        self.space.metric()
    }

    /// The place of the map's point `point`; an error unless it fits the map's metric and lies
    /// within its bounds.
    pub fn checked_position(&self, point: MapPoint) -> Result<Vec3, MapPointError> {
        point.place(self.metric(), self.bounds)
    }

    /// The point in space of the position `position`: on a planar map, its ground's height at it
    /// raised by the position's height above the ground (04-capabilities/00-overview.md, The sim's
    /// ground is the map's height grid); on a spatial map, the position itself.
    pub fn point(&self, position: Vec3) -> Vec3 {
        match &self.space {
            MapSpace::Planar(ground) => Vec3::new(
                position.x,
                ground.height(position.x, position.z) + position.y,
                position.z,
            ),
            MapSpace::Spatial => position,
        }
    }

    /// The position of the point in space `point`, as `point` gives it back.
    pub fn position(&self, point: Vec3) -> Vec3 {
        match &self.space {
            MapSpace::Planar(ground) => {
                let height = ground.height(point.x, point.z);
                Vec3::new(point.x, point.y - height, point.z)
            }
            MapSpace::Spatial => point,
        }
    }

    /// The highest of the ground's coarse cells the line from `a` to `b` crosses on the ground
    /// plane, as `Ground::highest_along` finds it; none on a spatial map, which has no ground.
    pub fn highest_along(&self, a: Vec3, b: Vec3) -> Option<Num> {
        match &self.space {
            MapSpace::Planar(ground) => ground.highest_along([a.x, a.z], [b.x, b.z]),
            MapSpace::Spatial => None,
        }
    }
}

impl MapBook {
    /// The map's space and bounds: on a planar map, its ground of its height grid, which the load
    /// gives when the map has one, as its `[ground]` reads it, flat with no grid; a spatial map's
    /// grid is its client's terrain alone. An error for a `[ground]` with no grid or on a spatial
    /// map, or coarse cells that do not fit the grid.
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
        let metric = sections
            .get::<MetricData>()
            .map_or_else(Metric::default, |metric| metric.0);
        let data = sections.get::<GroundData>();
        let space = match (metric, heights, data) {
            (Metric::Spatial, _, Some(_)) => return Err(GroundLoadError::Spatial),
            (Metric::Spatial, _, None) => MapSpace::Spatial,
            (Metric::Planar, None, None) => MapSpace::default(),
            (Metric::Planar, None, Some(_)) => return Err(GroundLoadError::NoHeights),
            (Metric::Planar, Some(grid), data) => {
                let GroundData { split, high } = data.copied().unwrap_or_default();
                let ground = Ground::new(grid.clone(), split, high);
                MapSpace::Planar(ground.map_err(GroundLoadError::Ground)?)
            }
        };
        Ok(Self { space, bounds })
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
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}

/// A map's `[bounds]`, which every map gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BoundsData(pub Bounds);

impl Section for BoundsData {
    const TABLE: SectionTable = SectionTable::Map;
    const KEY: &'static str = "bounds";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ground::CellSplit;

    /// A spatial map holds no sim ground: its height grid is its client's terrain alone, and a
    /// `[ground]` of it is refused; a planar map of the same grid stands on it.
    #[test]
    fn a_spatial_map_holds_no_ground_and_refuses_a_ground_section() {
        let bounds = Bounds::new([Num::ZERO; 2], [Num::int(10); 2]).unwrap();
        let grid = HeightGrid::new([Num::ZERO; 2], Num::int(10), Num::ONE, 2, vec![4; 4]).unwrap();
        let sections = |metric, ground: Option<GroundData>| {
            let sections = Sections::default()
                .with(BoundsData(bounds))
                .with(MetricData(metric));
            match ground {
                Some(ground) => sections.with(ground),
                None => sections,
            }
        };
        let ground = GroundData::default();
        let spatial = MapBook::of(&sections(Metric::Spatial, None), Some(&grid)).unwrap();
        assert_eq!(spatial.space, MapSpace::Spatial);
        assert_eq!(
            MapBook::of(&sections(Metric::Spatial, Some(ground)), Some(&grid)),
            Err(GroundLoadError::Spatial)
        );
        let planar = MapBook::of(&sections(Metric::Planar, Some(ground)), Some(&grid)).unwrap();
        let built = Ground::new(grid, CellSplit::default(), None).unwrap();
        assert_eq!(planar.space, MapSpace::Planar(built));
        // The planar map's point stands 4 m up its ground; the spatial map's is its position.
        let position = Vec3::new(Num::int(5), Num::ONE, Num::int(5));
        assert_eq!(planar.point(position).y, Num::int(5));
        assert_eq!(spatial.point(position), position);
    }
}
