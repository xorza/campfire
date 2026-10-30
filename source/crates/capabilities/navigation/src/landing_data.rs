use std::num::NonZeroU32;

use campfire_geometry::Radius;
use serde::Deserialize;

/// How a flier finds the point it lands at, of the mode's `[navigation.landing]`: the radius of
/// the circle the first point its search takes must clear, the gap between the search's rings,
/// and the cells of the spiral that then looks about that point for one its whole body clears;
/// each none for the flier's own radius, the pathing grid's cell and every cell of the grid
/// (riders-and-upgrades.md, A flier lands where Zero Hour's transport lands).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LandingData {
    #[serde(default)]
    pub probe: Option<Radius>,
    #[serde(default)]
    pub ring: Option<Radius>,
    #[serde(default)]
    pub cells: Option<NonZeroU32>,
}
