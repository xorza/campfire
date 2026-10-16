use bevy_ecs::resource::Resource;
use campfire_declare::Book;
use campfire_geometry::{Bounds, Metric};
use campfire_math::Vec3;
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
}

impl Book for MapBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &View) {}
}
