//! Packages: their files, as a source gives their bytes, the schemas of those files, and the
//! checks a mode and every package it depends on pass before a match loads them. Reading a
//! package's directory on disk is `campfire-package-disk`'s, outside the deterministic core.

// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (09-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]

mod avatar_unit;
mod data_override;
mod dependent;
mod error;
mod file_index;
mod files;
mod language;
mod locale_file;
mod locale_package;
mod mode_packages;
mod package;
mod package_files;
mod package_reader;
mod package_source;
mod package_text;
mod package_view;
mod package_walk;
mod rules_package;
mod texts;

pub use crate::avatar_unit::AvatarUnit;
pub use crate::data_override::DataOverride;
pub use crate::dependent::{Dependent, DependentKind};
pub use crate::error::load_problem::LoadProblem;
pub use crate::error::locale_problem::LocaleProblem;
pub use crate::error::place::Place;
pub use crate::error::script_problem::ScriptProblem;
pub use crate::error::{ContentError, LoadError, PackageRef};
pub use crate::file_index::{EncodedIndex, FileIndex};
pub use crate::files::dependency::Dependency;
pub use crate::files::dependency_path::DependencyPath;
pub use crate::files::mode_manifest::ModeManifest;
pub use crate::files::package_header::PackageHeader;
pub use crate::files::package_name::PackageName;
pub use crate::files::rate_bounds::RateBounds;
pub use crate::files::rules_manifest::RulesNeeds;
pub use crate::files::tick_range::TickRange;
pub use crate::language::Language;
pub use crate::locale_package::LocalePackage;
pub use crate::mode_packages::ModePackages;
pub use crate::package::{Package, Script};
pub use crate::package_files::PackageFiles;
pub use crate::package_reader::PackageReader;
pub use crate::package_source::PackageSource;
pub use crate::package_text::PackageText;
pub use crate::package_view::{PackageView, ViewKind};
pub use crate::package_walk::{HashedFile, PackageWalk};
pub use crate::rules_package::RulesPackage;
pub use crate::texts::Texts;
