use campfire_geometry::Metric;
use campfire_math::Vec3;
use campfire_sim::Position;
use campfire_values::Scalar;
use serde::Deserialize;

/// A point of the map, in meters: `[x, z]` on the ground plane of a planar map, `[x, y, z]` on a
/// spatial one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum MapPoint {
    Ground([Scalar; 2]),
    Space([Scalar; 3]),
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
