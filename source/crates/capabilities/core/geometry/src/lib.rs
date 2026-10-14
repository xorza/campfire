//! Geometry on the ground plane and in space, exact in `Num`: the map's metric and bounds, the
//! bodies' shapes and boxes, polygons, the grid of cells, and the exact distances and shares of a
//! path their tests compute.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod approach;
mod body_bound;
mod body_box;
mod body_grid;
mod bounds;
mod cell_key;
mod cell_levels;
mod grid;
mod halves;
mod metric;
mod polygon;
mod shape;

#[cfg(feature = "internals")]
mod kernel_scene;

pub use crate::approach::Approach;
pub use crate::body_bound::MAX_BOUND;
pub use crate::body_box::BodyBox;
pub use crate::body_grid::{BodyGrid, Placed};
pub use crate::bounds::Bounds;
pub use crate::cell_key::CellKey;
pub use crate::cell_levels::CellLevels;
pub use crate::grid::Grid;
pub use crate::halves::Halves;
pub use crate::metric::Metric;
pub use crate::polygon::Polygon;
pub use crate::polygon::error::PolygonError;
pub use crate::shape::Shape;

#[cfg(feature = "internals")]
pub mod bench {
    use criterion::Criterion;

    use crate::{body_box, body_grid, grid, shape};

    /// Runs each bench of the crate whose id criterion's filter takes.
    pub fn run(c: &mut Criterion) {
        body_box::bench::body_box(c);
        shape::bench::shape(c);
        grid::bench::grid(c);
        body_grid::bench::body_grid(c);
    }
}

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::body_box::bench::COUNT;
    pub use crate::kernel_scene::{Density, KernelScene, SceneBody};
}
