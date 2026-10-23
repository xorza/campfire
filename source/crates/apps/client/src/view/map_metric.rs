use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Res, SystemParam};
use campfire_geometry::Metric;

use crate::view::ground_heights::GroundHeights;

/// How the match's map reads a position: above its ground, or as a point in space.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MapMetric(pub(crate) Metric);

/// The drawn heights a sim position stands above, as a frame reads them: the ground's on a planar
/// map, where a position's height is above its ground; none on a spatial map, where a position is
/// a point in space, or on a map of no heightmap (04-capabilities/00-overview.md, The sim's ground
/// is the map's height grid).
#[derive(SystemParam, Debug)]
pub(crate) struct Lifts<'w> {
    metric: Res<'w, MapMetric>,
    heights: Option<Res<'w, GroundHeights>>,
}

impl MapMetric {
    /// The drawn heights a sim position stands above on the map of `heights`, as `Lifts` gives
    /// them.
    pub(crate) const fn lifts(self, heights: Option<&GroundHeights>) -> Option<&GroundHeights> {
        match self.0 {
            Metric::Planar => heights,
            Metric::Spatial => None,
        }
    }
}

impl Lifts<'_> {
    pub(crate) fn get(&self) -> Option<&GroundHeights> {
        self.metric.lifts(self.heights.as_deref())
    }
}
