//! Geometry on the ground plane and in space, exact in `Num`: the map's metric and bounds, the
//! bodies' shapes and boxes, polygons, the grid of cells, and the exact distances and shares of a
//! path their tests compute.

// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]

mod approach;
mod body_bound;
mod body_box;
mod body_grid;
mod bounds;
mod cell_key;
mod cell_levels;
mod grid;
mod grid_search;
mod halves;
mod metric;
mod placed;
mod polygon;
mod radius;
mod row_index;
mod shape;
mod volume;

pub use crate::body_bound::MAX_BOUND;
pub use crate::body_box::BodyBox;
pub use crate::body_grid::{BodyGrid, Indexed};
pub use crate::bounds::Bounds;
pub use crate::cell_key::CellKey;
pub use crate::cell_levels::CellLevels;
pub use crate::grid::{Grid, RunChange};
pub use crate::grid_search::GridSearch;
pub use crate::metric::Metric;
pub use crate::placed::Placed;
pub use crate::polygon::Polygon;
pub use crate::polygon::error::PolygonError;
pub use crate::radius::Radius;
pub use crate::radius::error::NotRadius;
pub use crate::row_index::{LayerView, RowEntries, RowIndex};
pub use crate::shape::Shape;
pub use crate::volume::Volume;

#[cfg(feature = "internals")]
pub mod bench {
    use criterion::Criterion;

    use crate::{body_box, body_grid, grid, placed};

    /// Runs each bench of the crate whose id criterion's filter takes.
    pub fn run(c: &mut Criterion) {
        body_box::bench::body_box(c);
        placed::bench::placed(c);
        grid::bench::grid(c);
        body_grid::bench::body_grid(c);
    }
}

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::approach::internals::path_comes_closer;
    pub use crate::kernel_scene::{Density, KernelScene, SceneBody};
}

#[cfg(feature = "internals")]
mod kernel_scene;
