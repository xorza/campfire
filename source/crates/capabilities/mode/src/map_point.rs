use campfire_geometry::Metric;
use campfire_math::{Num, Vec3};
use campfire_sim::Position;
use serde::Deserialize;

/// A point of the map, in meters: `[x, z]` on the ground plane of a planar map, `[x, y, z]` on a
/// spatial one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum MapPoint {
    Ground([Num; 2]),
    Space([Num; 3]),
}

impl MapPoint {
    /// The point as a position, a ground point at height 0; `None` beyond the world's bound.
    pub const fn position(self) -> Option<Position> {
        let [x, y, z] = match self {
            MapPoint::Ground([x, z]) => [x, Num::ZERO, z],
            MapPoint::Space(point) => point,
        };
        Position::new(Vec3::new(x, y, z))
    }

    /// Whether it has the shape of a point of a map of `metric`.
    pub const fn fits(self, metric: Metric) -> bool {
        matches!(
            (self, metric),
            (MapPoint::Ground(_), Metric::Planar) | (MapPoint::Space(_), Metric::Spatial)
        )
    }
}
