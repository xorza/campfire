use campfire_common::PackageFingerprint;

use crate::error::ContentError;
use crate::package_files::PackageFiles;

/// Packages by their fingerprints, as a verifier's store holds them.
pub trait PackageFinder {
    /// The files of the package of `fingerprint`, read now; none when it holds none, and an error
    /// for a package that does not read, or no longer has that fingerprint.
    fn find(&self, fingerprint: PackageFingerprint) -> Option<Result<PackageFiles, ContentError>>;
}
