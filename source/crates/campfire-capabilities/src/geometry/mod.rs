//! Geometry on the ground plane and in space, exact in `Num`: the map's metric and bounds, the
//! bodies' shapes and boxes, polygons, the grid of cells, and the distances and fractions their
//! tests compute.

pub(crate) mod approach;
pub(crate) mod body_box;
pub(crate) mod bounds;
pub(crate) mod fraction;
pub(crate) mod grid;
pub(crate) mod metric;
pub(crate) mod polygon;
pub(crate) mod shape;
pub(crate) mod squared_distance;

#[cfg(any(test, feature = "internals"))]
pub(crate) mod kernel_scene;
