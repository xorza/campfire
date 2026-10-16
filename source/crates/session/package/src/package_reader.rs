use std::str;
use std::sync::Arc;

use campfire_codec::Toml;
use campfire_common::{ContentHash, PackageFingerprint};
use campfire_values::PackagePath;
use serde::de::DeserializeOwned;

use crate::error::ContentError;
use crate::file_index::{FileIndex, FileRow};
use crate::package_source::PackageSource;

/// A package's files as its client reads them, on demand: its index, and the source it reads
/// each file from, checked against the file's row, so a file read is the one the fingerprint
/// names.
#[derive(Debug, Clone)]
pub struct PackageReader {
    /// Shared by every clone, as a mode's packages each keep their reader.
    index: Arc<FileIndex>,
    source: Arc<dyn PackageSource>,
}

impl PackageReader {
    pub(crate) fn new(index: Arc<FileIndex>, source: Arc<dyn PackageSource>) -> Self {
        Self { index, source }
    }

    pub fn fingerprint(&self) -> PackageFingerprint {
        self.index.fingerprint()
    }

    pub(crate) fn index(&self) -> &FileIndex {
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

    /// The TOML data file at `path`, which a load does not keep, read now, checked against its
    /// row, and read as a `T`, as a session's map is.
    pub fn read_data<T: DeserializeOwned>(&self, path: &PackagePath) -> Result<T, ContentError> {
        let bytes = self.read_file(path)?;
        let text = str::from_utf8(&bytes).map_err(|error| ContentError::NotText {
            path: path.clone(),
            error,
        })?;
        Toml::parse(text).map_err(|error| ContentError::Data {
            path: path.clone(),
            error,
        })
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
        if size != row.size || ContentHash::of(&bytes) != row.sha256 {
            return Err(ContentError::Changed { path: path.clone() });
        }
        Ok(bytes)
    }
}
