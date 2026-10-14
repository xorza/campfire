use bevy_ecs::resource::Resource;
use campfire_declare::Book;
use campfire_geometry::{Bounds, Metric};
use campfire_script::View;
use campfire_sim::Capability;

/// The map's measure of distance and its bounds; the world's bounds before a map names its own.
/// Package data, not state.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapBook {
    pub metric: Metric,
    pub bounds: Bounds,
}

impl Default for MapBook {
    fn default() -> MapBook {
        MapBook {
            metric: Metric::default(),
            bounds: Bounds::WORLD,
        }
    }
}

impl Book for MapBook {
    const OWNER: Option<Capability> = None;
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &View) {}
}
