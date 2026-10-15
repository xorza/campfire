//! The client's data files of a package, which the sim never reads, and which only the import,
//! that writes them, and the client, that draws them, depend on: how its camera moves, how its
//! unit types and its maps' client objects look, the materials that override its models' own, the
//! light its maps are drawn in, their terrain and its ground heights, and the objects of each map
//! the client alone draws.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod camera_file;
mod client_objects;
mod client_units;
mod error;
mod height_grid;
mod map_lights;
mod material_file;
mod terrain_file;
mod texture_levels;

pub use crate::camera_file::{CameraFile, CameraHeight};
pub use crate::client_objects::{ClientObject, ClientObjects};
pub use crate::client_units::{ClientModel, ClientUnit, ClientUnits, ModelScale};
pub use crate::error::{TerrainFileError, TextureLevelsError};
pub use crate::height_grid::HeightGrid;
pub use crate::map_lights::{MapLight, MapLights};
pub use crate::material_file::{Blend, MaterialFile, MaterialTexture};
pub use crate::terrain_file::{
    BorderWrap, GroundQuad, Overlay, TerrainFile, TerrainQuads, TerrainSurface, TerrainTexture,
    TextureCopy,
};
pub use crate::texture_levels::TextureLevels;
