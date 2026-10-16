//! Packages on disk: reading a package's directory into the files the core checks, the store of
//! packages by fingerprint a verifier holds, and the writer of a new package. Every file goes
//! through `store`, which the deterministic core of `campfire-package` does not reach.

mod error;
#[cfg(feature = "internals")]
mod harness;
mod package_dir;
mod package_place;
mod package_store;
mod package_writer;

pub use crate::error::{IndexError, StoreError, WriteError};
pub use crate::package_dir::PackageDir;
pub use crate::package_store::{PackageStore, StoreFailure};
pub use crate::package_writer::PackageWriter;

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::harness::package_tree::{Edit, PackageTree};
    pub use crate::harness::test_mode::TestMode;
    pub use crate::harness::tree_dir::TreeDir;
}
