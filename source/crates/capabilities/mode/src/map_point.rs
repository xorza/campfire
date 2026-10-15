use campfire_geometry::Metric;
use campfire_math::{Num, Vec3};
use campfire_sim::Position;
use serde::{Deserialize, Serialize};

/// A point of the map, in meters: `[x, z]` on the ground plane of a planar map, `[x, y, z]` on a
/// spatial one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MapPoint {
    Ground([Num; 2]),
    Space([Num; 3]),
}

impl MapPoint {
    /// The place of the point, a ground point at height 0; `None` beyond the world's bound.
    pub fn pos(self) -> Option<Vec3> {
        let [x, y, z] = match self {
            MapPoint::Ground([x, z]) => [x, Num::ZERO, z],
            MapPoint::Space(point) => point,
        };
        let point = Vec3::new(x, y, z);
        Position::from(point).within_bound().then_some(point)
    }

    /// Whether it has the shape of a point of a map of `metric`.
    pub const fn fits(self, metric: Metric) -> bool {
        matches!(
            (self, metric),
            (MapPoint::Ground(_), Metric::Planar) | (MapPoint::Space(_), Metric::Spatial)
        )
    }
}
