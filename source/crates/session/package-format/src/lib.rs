//! A package's files as bytes: the index of its files, the source that gives their bytes and
//! the reader that checks them against the index, the walk that indexes a tree, and where a
//! package and its dependencies are. It holds no schema of a file, so indexing a package
//! compiles no engine.

mod content_hash;
mod dependency_path;
mod error;
mod file_index;
mod package_files;
mod package_finder;
mod package_place;
mod package_reader;
mod package_source;
mod package_walk;

pub use crate::content_hash::{ContentHash, ContentHashMeaning, ContentHasher};
pub use crate::dependency_path::DependencyPath;
pub use crate::error::ContentError;
pub use crate::file_index::{EncodedIndex, FileIndex};
pub use crate::package_files::PackageFiles;
pub use crate::package_finder::PackageFinder;
pub use crate::package_place::PackagePlace;
pub use crate::package_reader::PackageReader;
pub use crate::package_source::PackageSource;
pub use crate::package_walk::{HashedFile, PackageWalk};
