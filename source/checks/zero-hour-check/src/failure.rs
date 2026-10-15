use campfire_common::MapName;
use campfire_log::ErrorReport;
use campfire_package::LoadError;
use derive_more::Display;

use crate::{TERRAIN, WALLS};

/// What the import holds other than the original gives it: what Tournament Desert holds, or a
/// map that does not load in the imported mode.
#[derive(Debug, Display)]
pub(crate) enum Failure {
    /// Its heights are not 270 × 340 samples.
    #[display("its heights are {columns} × {rows} samples, not 270 × 340")]
    Heights { columns: u32, rows: u32 },
    /// It places other than 142 units of 41 types.
    #[display("it places {units} units of {types} types, not 142 of 41")]
    Units { units: usize, types: usize },
    /// It has other than 438 client objects of 22 looks.
    #[display("it has {objects} client objects of {looks} looks, not 438 of 22")]
    ClientObjects { objects: usize, looks: usize },
    /// It has other than 28 markers.
    #[display("it has {_0} markers, not 28")]
    Markers(usize),
    /// Its terrain file holds other than its pinned quads, overlays, copies and wraps.
    #[display(
        "its terrain has {} quads, {} overlays, {} copies and {} wraps, not {}, {}, {} and {}",
        _0.quads, _0.overlays, _0.copies, _0.wraps,
        TERRAIN.quads, TERRAIN.overlays, TERRAIN.copies, TERRAIN.wraps
    )]
    Terrain(TerrainDrawn),
    /// Cells it draws have a tile with no texture.
    #[display("{_0} of its drawn cells have a tile with no texture")]
    Untextured(usize),
    /// Its grown cliffs merge into other than its pinned walls.
    #[display("it has {_0} walls, not {WALLS}")]
    Walls(usize),
    /// A map of the import does not load in the imported mode.
    #[display("map {map} does not load: {}", ErrorReport::of(error))]
    MapLoad { map: MapName, error: LoadError },
}

/// How many ground quads, overlays, texture copies and border wraps a terrain file holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerrainDrawn {
    pub(crate) quads: usize,
    pub(crate) overlays: usize,
    pub(crate) copies: usize,
    pub(crate) wraps: usize,
}
