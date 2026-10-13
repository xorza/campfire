use serde::Deserialize;

use crate::geometry::bounds::Bounds;
use crate::geometry::metric::Metric;
use crate::mode::map_point::MapPoint;

/// A box of the map, from `min` to `max`, `min` below `max` on every axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionData {
    pub min: MapPoint,
    pub max: MapPoint,
}

impl RegionData {
    /// The box on the ground plane it gives, when its points fit `metric` and lie within
    /// `bounds`, `min` below `max` on every axis of the metric.
    pub(crate) fn region(self, metric: Metric, bounds: Bounds) -> Option<Bounds> {
        let (min, max) = (self.min.position()?, self.max.position()?);
        let (low, high) = (min.get(), max.get());
        let below = metric == Metric::Planar || low.y < high.y;
        let fits = self.min.fits(metric) && self.max.fits(metric);
        let within = bounds.contains(min) && bounds.contains(max);
        Bounds::new([low.x, low.z], [high.x, high.z]).filter(|_| fits && below && within)
    }
}
