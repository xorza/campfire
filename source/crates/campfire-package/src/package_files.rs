use std::collections::BTreeMap;

use campfire_capabilities::PackagePath;
use campfire_common::{Fingerprint, Toml};
use serde::de::DeserializeOwned;

use crate::error::ContentError;
use crate::file_index::FileIndex;
use crate::package_dir::PackageDir;

/// A package as a load reads it: its index, and the files a load reads, read once into memory
/// and checked against their rows, each file's bytes by its path in the package. A load parses
/// only these bytes, so what it parses is what the fingerprint names; any other file it reads on
/// demand, checked the same way.
#[derive(Debug, Clone)]
pub struct PackageFiles {
    /// In the order of their paths.
    files: BTreeMap<PackagePath, Vec<u8>>,
    index: FileIndex,
    dir: PackageDir,
}

impl PackageFiles {
    /// The package in `dir` of `index`, and `files`, the ones a load reads.
    pub(crate) const fn new(
        files: BTreeMap<PackagePath, Vec<u8>>,
        index: FileIndex,
        dir: PackageDir,
    ) -> PackageFiles {
        PackageFiles { files, index, dir }
    }

    pub const fn fingerprint(&self) -> Fingerprint {
        self.index.fingerprint()
    }

    /// The files under the directory `dir` that a load reads, in the order of their paths.
    pub(crate) fn files_under<'a>(&'a self, dir: &'a str) -> impl Iterator<Item = &'a PackagePath> {
        self.files.keys().filter(move |path| path.is_under(dir))
    }

    /// The TOML data file at `path`, read as a `T`.
    pub fn read_data<T: DeserializeOwned>(&self, path: &PackagePath) -> Result<T, ContentError> {
        Toml::parse(self.read_text(path)?).map_err(|error| ContentError::Data {
            path: path.clone(),
            error,
        })
    }

    /// The text of the file at `path` that a load reads, such as a script's source.
    pub fn read_text(&self, path: &PackagePath) -> Result<&str, ContentError> {
        let bytes = self
            .files
            .get(path)
            .ok_or_else(|| ContentError::Missing { path: path.clone() })?;
        PackageFiles::text(path, bytes)
    }

    /// The TOML data file at `path`, which a load does not keep, read now and checked against its
    /// row, as a session's map is.
    pub(crate) fn read_file_data<T: DeserializeOwned>(
        &self,
        path: &PackagePath,
    ) -> Result<T, ContentError> {
        let bytes = self.read_file(path)?;
        Toml::parse(PackageFiles::text(path, &bytes)?).map_err(|error| ContentError::Data {
            path: path.clone(),
            error,
        })
    }

    pub(crate) const fn index(&self) -> &FileIndex {
        &self.index
    }

    fn text<'a>(path: &PackagePath, bytes: &'a [u8]) -> Result<&'a str, ContentError> {
        str::from_utf8(bytes).map_err(|error| ContentError::NotText {
            path: path.clone(),
            error,
        })
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
