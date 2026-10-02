//! Packages: reading their files, the schemas of those files, and the checks a mode and every
//! package it depends on pass before a match loads them.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod avatar_unit;
mod dependent;
mod error;
mod files;
mod load_check;
mod locale_file;
mod locale_package;
mod mode_packages;
mod package;
mod package_dir;
mod package_files;
mod package_index;
mod package_store;
mod package_text;
mod package_view;
mod script_facts;
mod texts;

pub use error::{
    ChoiceProblem, ContentError, CtxMisuse, DeliveryProblem, EffectProblem, Limit, LoadError,
    LoadProblem, LocaleProblem, PackageRef, Place, ScriptProblem, StoreError,
};
pub use files::avatar_data::AvatarData;
pub use files::tick_range::TickRange;

pub use locale_package::LocalePackage;
pub use mode_packages::ModePackages;
pub use package::Script;
pub use package_dir::PackageDir;
pub use package_files::PackageFiles;

pub use package_store::{PackageStore, StoreFailure};

pub use texts::Texts;

/// The tag of this engine release: what a session's terms name, so a replay runs the code that
/// recorded it.
pub const RELEASE: &str = env!("CARGO_PKG_VERSION");
