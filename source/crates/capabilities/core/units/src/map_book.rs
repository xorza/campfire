use bevy_ecs::resource::Resource;
use campfire_declare::Book;
use campfire_geometry::{Bounds, Metric};
use campfire_math::{Num, Vec3};
use campfire_script::View;

use crate::ground::Ground;

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
    /// The point in space of the position `pos`: on a planar map, its ground's height at it
    /// raised by the position's height above the ground (20-zero-hour-projectiles.md, The sim's
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

impl Book for MapBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &View) {}
}
