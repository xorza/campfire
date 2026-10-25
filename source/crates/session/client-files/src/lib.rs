//! The client's data files of a package, which the sim never reads, and which only the import, that
//! writes them, and the client, that draws them, depend on: how its camera moves, which keys do
//! what, which units show their bars, how the pointer picks its units, its teams' colors, how its
//! unit types and its maps' client objects look, what its models' materials and animations say
//! past glTF's own, the light its maps are drawn in, their terrain and its ground heights, and the
//! objects of each map the client alone draws.

// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]

mod animation_extras;
mod camera_file;
mod client_file;
mod client_model;
mod client_objects;
mod client_path;
mod client_unit;
mod client_units;
mod condition_test;
mod error;
mod hud_file;
mod keys_file;
mod ktx2_image;
mod map_conditions;
mod map_lights;
mod material_extras;
mod object_look;
mod picking_file;
mod state_animation;
mod team_colors;
mod terrain_file;
mod texture_format;
mod texture_levels;

pub use crate::animation_extras::{AnimationExtras, HiddenFrames};
pub use crate::camera_file::{CameraFile, CameraHeight};
pub use crate::client_file::{ClientFile, ClientFormat};
pub use crate::client_model::{ClientModel, ModelScale, ModelTop};
pub use crate::client_objects::{ClientObject, ClientObjects};
pub use crate::client_unit::{ClientUnit, LookDraw, LookState, LookTransition, LookVariation};
pub use crate::client_units::ClientUnits;
pub use crate::condition_test::{ConditionTest, LifeShare};
pub use crate::error::{ClientFileError, TeamColorsError, TerrainFileError, TextureLevelsError};
pub use crate::hud_file::{HudFile, ShownBars};
pub use crate::keys_file::{Chord, ChordLabel, Key, KeyModifiers, KeysFile, Press};
pub use crate::ktx2_image::Ktx2Image;
pub use crate::map_conditions::MapConditions;
pub use crate::map_lights::{MapLight, MapLights};
pub use crate::material_extras::{Blend, MaterialExtras, TeamPaint};
pub use crate::object_look::{LookVariant, ObjectLook};
pub use crate::picking_file::{PickingFile, PixelRadius};
pub use crate::state_animation::{
    AnimationMode, SpeedRange, StartFrame, StateAnimation, StateClip, StatePlay,
};
pub use crate::team_colors::TeamColors;
pub use crate::terrain_file::{
    BorderWrap, GroundQuad, Overlay, TerrainFile, TerrainQuads, TerrainSurface, TerrainTexture,
    TextureCopy,
};
pub use crate::texture_format::TextureFormat;
pub use crate::texture_levels::TextureLevels;
