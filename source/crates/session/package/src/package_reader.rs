use std::sync::Arc;

use campfire_common::Fingerprint;
use campfire_values::PackagePath;
use sha2::{Digest, Sha256};

use crate::error::ContentError;
use crate::file_index::{FileIndex, FileRow};
use crate::package_source::PackageSource;

/// A package's files as its client reads them, on demand: its index, and the source it reads
/// each file from, checked against the file's row, so a file read is the one the fingerprint
/// names.
#[derive(Debug, Clone)]
pub struct PackageReader {
    index: FileIndex,
    source: Arc<dyn PackageSource>,
}

impl PackageReader {
    pub(crate) fn new(index: FileIndex, source: Arc<dyn PackageSource>) -> PackageReader {
        PackageReader { index, source }
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
        self.read_row(path, row)
    }

    /// The bytes of the file at `path`, whose row is `row`, read no further than its row's size
    /// and refused when they differ from it.
    pub(crate) fn read_row(
        &self,
        path: &PackagePath,
        row: FileRow,
    ) -> Result<Vec<u8>, ContentError> {
        let bytes = self.source.read(path, row.size)?;
        let size = u64::try_from(bytes.len()).expect("a file length fits u64");
        if size != row.size || <[u8; 32]>::from(Sha256::digest(&bytes)) != row.sha256 {
            return Err(ContentError::Changed { path: path.clone() });
        }
        Ok(bytes)
    }
}
