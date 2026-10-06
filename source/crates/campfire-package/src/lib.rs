//! Packages: reading their files, the schemas of those files, and the checks a mode and every
//! package it depends on pass before a match loads them.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod avatar_unit;
mod dependent;
mod error;
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
mod package_store;
mod package_text;
mod package_view;
mod script_facts;
mod texts;

pub use crate::error::{
    ChoiceProblem, ContentError, CtxMisuse, DeliveryProblem, EffectProblem, ItemProblem, Limit,
    LoadError, LoadProblem, LocaleProblem, PackageRef, Place, ScriptProblem, StoreError,
};
pub use crate::files::avatar_data::AvatarData;
pub use crate::files::tick_range::TickRange;

pub use crate::language::Language;
pub use crate::locale_package::LocalePackage;
pub use crate::message_id::MessageId;
pub use crate::mode_packages::ModePackages;
pub use crate::modifier_ways::Way;
pub use crate::package::Script;
pub use crate::package_dir::PackageDir;
pub use crate::package_files::PackageFiles;

pub use crate::package_store::{PackageStore, StoreFailure};

pub use crate::texts::Texts;

/// The tag of this engine release: what a session's terms name, so a replay runs the code that
/// recorded it.
pub const RELEASE: &str = env!("CARGO_PKG_VERSION");
