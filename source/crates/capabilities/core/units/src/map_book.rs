use bevy_ecs::resource::Resource;
use campfire_declare::Book;
use campfire_geometry::{Bounds, Metric};
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

impl Book for MapBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &View) {}
}
