use crate::units::layer::Layer;
use crate::values::polygon::Polygon;

/// A wall of the map: on its layer, every cell whose center its area holds, edge included, is
/// blocked to every walker, as a cliff or the jungle's trees are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Wall {
    pub(crate) layer: Layer,
    pub(crate) area: Polygon,
}
