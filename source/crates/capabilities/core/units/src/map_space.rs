use campfire_geometry::Metric;

use crate::ground::Ground;

/// A map's space: a plane over its ground, whose heights the sim reads, or 3D space, where a
/// position is its own point and the sim reads no ground, so a spatial map holds none
/// (04-capabilities/00-overview.md, A spatial map has no sim ground).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapSpace {
    Planar(Ground),
    Spatial,
}

impl MapSpace {
    /// The measure of distance of its maps.
    pub const fn metric(&self) -> Metric {
        match self {
            Self::Planar(_) => Metric::Planar,
            Self::Spatial => Metric::Spatial,
        }
    }
}

impl Default for MapSpace {
    fn default() -> Self {
        Self::Planar(Ground::default())
    }
}

/// The space of `metric` on a flat ground, as a map with no height grid has.
impl From<Metric> for MapSpace {
    fn from(metric: Metric) -> Self {
        match metric {
            Metric::Planar => Self::default(),
            Metric::Spatial => Self::Spatial,
        }
    }
}
