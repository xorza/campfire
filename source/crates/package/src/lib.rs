//! Packages: reading their files, the schemas of those files, and the checks a mode and every
//! package it depends on pass before a match loads them.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod error;
mod files;
mod load_check;
mod mode_packages;
mod package;
mod package_dir;
mod package_store;
mod script_facts;

pub use error::{ContentError, CtxMisuse, LoadError, LoadProblem, Place, StoreError};
pub use files::hero_data::{HeroData, ResourceKind};
pub use files::manifest::{
    Backends, CollisionBackend, Dependency, Manifest, ModeManifest, PackageHeader,
    PathfindingBackend, TickRange, VisibilityBackend,
};
pub use files::spells_data::SpellsData;
pub use files::units_data::{UnitTypeFile, UnitsData, VisionData};
pub use files::version::Version;
pub use mode_packages::{Content, Dependent, ModePackages};
pub use package::{Package, Script};
pub use package_dir::PackageDir;
pub use package_store::PackageStore;

/// The tag of this engine release: what a package targets and a session's terms name.
pub const RELEASE: &str = env!("CARGO_PKG_VERSION");

/// `RELEASE` as the version a package's `engine` names.
const RELEASE_VERSION: Version = match Version::parse(RELEASE) {
    Some(version) => version,
    None => panic!("the crate version is major.minor.patch"),
};
