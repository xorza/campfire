use std::fmt;

use campfire_values::PackagePath;

use crate::error::ContentError;

/// Where a package's files come from, as the core reads them: a directory on disk, or a tree in
/// memory, as a test builds one. The core checks every byte against the package's index, so a
/// source only reads; it holds no rule of a package.
pub trait PackageSource: fmt::Debug + Send + Sync {
    /// The bytes of the package's index.
    fn read_index(&self) -> Result<Vec<u8>, ContentError>;

    /// An error unless the source holds each of `paths` by its exact name: a file system that
    /// ignores case, as macOS's and Windows' do by default, would open a name that differs only
    /// in case, which Linux holds as missing, so one package would load on one OS and fail on
    /// another.
    fn check_names(
        &self,
        paths: &mut dyn Iterator<Item = &PackagePath>,
    ) -> Result<(), ContentError>;

    /// The `size` bytes of the file at `path`: `ContentError::Changed` for a file of another
    /// size, and `ContentError::Missing` for none.
    fn read(&self, path: &PackagePath, size: u64) -> Result<Vec<u8>, ContentError>;
}
