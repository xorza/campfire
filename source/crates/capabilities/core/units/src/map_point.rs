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
            Self::Ground([x, z]) => [x, Num::ZERO, z],
            Self::Space(point) => point,
        };
        let point = Vec3::new(x, y, z);
        Position::from(point).within_bound().then_some(point)
    }

    /// Whether it has the shape of a point of a map of `metric`.
    pub const fn fits(self, metric: Metric) -> bool {
        matches!(
            (self, metric),
            (Self::Ground(_), Metric::Planar) | (Self::Space(_), Metric::Spatial)
        )
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_math::Num;

    use crate::map_point::MapPoint;

    impl MapPoint {
        pub const fn ground(x: i64, z: i64) -> Self {
            Self::Ground([Num::int(x), Num::int(z)])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
