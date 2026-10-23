//! Packages on disk: reading a package's directory into the files the core checks, the store of
//! packages by fingerprint a verifier holds, and the writer of a new package. Every file goes
//! through `store`, which the deterministic core does not reach, and a package's format alone:
//! a mode loads from these places through `campfire-package`.

mod error;
mod package_dir;
mod package_store;
mod package_writer;
mod read_bound;
#[cfg(feature = "internals")]
mod tree_dir;

pub use crate::error::{IndexError, WriteError};
pub use crate::package_dir::PackageDir;
pub use crate::package_store::{PackageStore, StoreFailure};
pub use crate::package_writer::PackageWriter;

/// A package tree in memory, for the tests of this crate and `campfire-test-packages`.
#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::tree_dir::TreeDir;
}
