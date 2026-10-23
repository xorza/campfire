use std::path::Path;
use std::sync::Arc;

use crate::dependency_path::DependencyPath;
use crate::error::ContentError;
use crate::package_files::PackageFiles;
use crate::package_source::PackageSource;

/// Where a package is: a directory on disk, or, in a test, a directory of a tree in memory. It
/// is the source of the package's files, and the places of the packages its manifest names are
/// its own joined to their paths.
pub trait PackagePlace: PackageSource + Clone + 'static {
    /// The place of the package at `relative` from this one's, as a manifest names a dependency.
    #[must_use]
    fn join(&self, relative: &DependencyPath) -> Self;

    /// The directory it names, by which an error names the package.
    fn dir(&self) -> &Path;

    /// The package's files, as the core reads and checks them: see `PackageFiles::read`.
    fn files(&self) -> Result<PackageFiles, ContentError> {
        PackageFiles::read(Arc::new(self.clone()))
    }
}
