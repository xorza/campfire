//! Packages: reading their files, the schemas of those files, and the checks a mode and every
//! package it depends on pass before a match loads them.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod avatar_unit;
mod client_data;
mod dependent;
mod error;
mod file_index;
mod files;
mod language;
mod load_check;
mod locale_file;
mod locale_package;
mod message_id;
mod mode_packages;
mod modifier_ways;
mod package;
mod package_dir;
mod package_files;
mod package_index;
mod package_reader;
mod package_store;
mod package_text;
mod package_view;
mod package_walk;
mod package_writer;
mod script_facts;
mod texts;
mod zero_hour;

pub use crate::client_data::camera_file::{CameraFile, CameraHeight};
pub use crate::client_data::client_units::{ClientModel, ClientUnit, ClientUnits};
pub use crate::client_data::map_lights::{MapLight, MapLights};
pub use crate::client_data::material_file::{Blend, MaterialFile, MaterialTexture};
pub use crate::error::box_problem::BoxProblem;
pub use crate::error::build_problem::BuildProblem;
pub use crate::error::choice_problem::ChoiceProblem;
pub use crate::error::ctx_misuse::CtxMisuse;
pub use crate::error::delivery_problem::DeliveryProblem;
pub use crate::error::effect_problem::EffectProblem;
pub use crate::error::gather_problem::GatherProblem;
pub use crate::error::item_problem::ItemProblem;
pub use crate::error::limit::Limit;
pub use crate::error::load_problem::LoadProblem;
pub use crate::error::locale_problem::LocaleProblem;
pub use crate::error::place::Place;
pub use crate::error::script_problem::ScriptProblem;
pub use crate::error::{ContentError, LoadError, PackageRef, StoreError, WriteError};
pub use crate::file_index::FileIndex;
pub use crate::files::avatar_data::AvatarData;
pub use crate::files::package_name::PackageName;
pub use crate::files::tick_range::TickRange;
pub use crate::language::Language;
pub use crate::locale_package::LocalePackage;
pub use crate::message_id::MessageId;
pub use crate::mode_packages::ModePackages;
pub use crate::modifier_ways::Way;
pub use crate::package::Script;
pub use crate::package_dir::PackageDir;
pub use crate::package_files::PackageFiles;
pub use crate::package_reader::PackageReader;
pub use crate::package_store::{PackageStore, StoreFailure};
pub use crate::package_writer::PackageWriter;
pub use crate::texts::Texts;
pub use crate::zero_hour::class_texture::ClassTexture;
pub use crate::zero_hour::error::{ClassTextureError, TerrainError};
pub use crate::zero_hour::game_data::GameData;
pub use crate::zero_hour::terrain::{
    BlendShape, BlendTile, CliffUv, Terrain, TerrainCell, TerrainLight, TerrainLighting,
    TerrainParts, TextureClass,
};
pub use crate::zero_hour::terrain_atlas::TerrainAtlas;

/// The tag of this engine release: what a session's terms name, so a replay runs the code that
/// recorded it.
pub const RELEASE: &str = env!("CARGO_PKG_VERSION");
