use campfire_geometry::Polygon;

use crate::layer::Layer;

/// A wall of the map: on its layer, every cell whose center its area holds, edge included, is
/// blocked to every walker, as a cliff or the jungle's trees are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wall {
    pub layer: Layer,
    pub area: Polygon,
}
