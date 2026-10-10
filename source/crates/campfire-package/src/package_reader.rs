use campfire_capabilities::PackagePath;
use campfire_common::Fingerprint;

use crate::error::ContentError;
use crate::file_index::FileIndex;
use crate::package_dir::PackageDir;

/// A package's files as its client reads them, on demand: its index, and the directory it reads
/// each file from, checked against the file's row, so a file read is the one the fingerprint
/// names.
#[derive(Debug, Clone)]
pub struct PackageReader {
    index: FileIndex,
    dir: PackageDir,
}

impl PackageReader {
    pub(crate) const fn new(index: FileIndex, dir: PackageDir) -> PackageReader {
        PackageReader { index, dir }
    }

    pub const fn fingerprint(&self) -> Fingerprint {
        self.index.fingerprint()
    }

    pub(crate) const fn index(&self) -> &FileIndex {
        &self.index
    }

    /// Every file under the directory `dir`, in the order of their paths, as its index lists them.
    pub fn files_under<'a>(&'a self, dir: &'a str) -> impl Iterator<Item = &'a PackagePath> {
        self.index
            .rows()
            .map(|(path, _)| path)
            .filter(move |path| path.is_under(dir))
    }

    /// The bytes of any file of the package, as an asset is read when it is drawn: read now, and
    /// checked against its row. A path the index does not list is missing.
    pub fn read_file(&self, path: &PackagePath) -> Result<Vec<u8>, ContentError> {
        let row = self
            .index
            .row(path)
            .ok_or_else(|| ContentError::Missing { path: path.clone() })?;
        self.dir.read_row(path, row)
    }
}
