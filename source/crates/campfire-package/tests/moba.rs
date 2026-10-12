//! The MOBA packages, read from disk, and in memory with edits made and their indexes written
//! again.

use std::path::PathBuf;

use campfire_common::MapName;
use campfire_package::internals::{Edit, PackageTree, TestMode};
use campfire_package::{LoadError, ModePackages, PackageDir};

/// The map of the MOBA's 3v3.
pub(crate) fn two_lanes() -> MapName {
    TestMode::Moba3v3.map()
}

/// The MOBA's 3v3 in `dir` on its map.
pub(crate) fn load(dir: &PackageDir) -> Result<ModePackages, LoadError> {
    ModePackages::from_package_dir(dir, &two_lanes())
}

pub(crate) fn moba() -> PathBuf {
    PackageDir::workspace("test/moba")
}

/// The MOBA packages in memory with `edits` made, each to a file by its path from their
/// root: their 3v3.
pub(crate) fn edited<'a>(edits: impl IntoIterator<Item = (&'a str, Edit<'a>)>) -> PackageDir {
    edited_at(TestMode::Moba3v3.root(), edits)
}

/// The MOBA packages in memory with `edits` made, each to a file by its path from their
/// root: the package at `root`.
pub(crate) fn edited_at<'a>(
    root: &str,
    edits: impl IntoIterator<Item = (&'a str, Edit<'a>)>,
) -> PackageDir {
    let mut tree = PackageTree::of(TestMode::Moba3v3);
    for (file, edit) in edits {
        tree.edit(file, edit);
    }
    tree.dir(root)
}
