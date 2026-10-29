use std::collections::BTreeMap;
use std::str;
use std::sync::Arc;

use campfire_codec::{Binary, Toml};
use campfire_common::PackageFingerprint;
use campfire_values::PackagePath;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::content_hash::ContentHash;
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
    /// Each row whose bytes the source holds at another path, by its path: the first path of its
    /// SHA-256 that the source holds.
    elsewhere: Arc<BTreeMap<PackagePath, PackagePath>>,
}

impl PackageReader {
    /// The reader of the package `source` holds, by its `index`: each row reads from the first
    /// path of its SHA-256, in the index's order, that the source holds by its exact name, so a
    /// package holds each SHA-256 once (05-protocol-spec.md, A package holds each SHA-256 once).
    /// `Missing` for the first row whose SHA-256 the source holds at no path.
    pub(crate) fn open(
        index: Arc<FileIndex>,
        source: Arc<dyn PackageSource>,
    ) -> Result<Self, ContentError> {
        let held = source.holds(&mut index.paths())?;
        debug_assert_eq!(
            held.len(),
            index.paths().count(),
            "a source answers each path"
        );
        let mut elsewhere = BTreeMap::new();
        {
            let mut firsts: BTreeMap<ContentHash, &PackagePath> = BTreeMap::new();
            for ((path, row), held) in index.rows().zip(held) {
                if held {
                    firsts.entry(row.sha256).or_insert(path);
                }
            }
            for (path, row) in index.rows() {
                let first = firsts
                    .get(&row.sha256)
                    .ok_or_else(|| ContentError::Missing { path: path.clone() })?;
                if *first != path {
                    elsewhere.insert(path.clone(), (*first).clone());
                }
            }
        }
        Ok(Self {
            index,
            source,
            elsewhere: Arc::new(elsewhere),
        })
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

    /// The binary data file at `path`, which a load does not keep, read now, checked against its
    /// row, and decoded as a `T`, as a map's height grid is.
    pub fn read_binary<T: Serialize + DeserializeOwned>(
        &self,
        path: &PackagePath,
    ) -> Result<T, ContentError> {
        let bytes = self.read_file(path)?;
        Binary::decode(&bytes).map_err(|error| ContentError::Binary {
            path: path.clone(),
            error,
        })
    }

    /// The bytes of the file at `path`, whose row is `row`, read from where the source holds
    /// them no further than its row's size, and refused, by the path read, when they differ from
    /// it.
    pub(crate) fn read_row(
        &self,
        path: &PackagePath,
        row: FileRow,
    ) -> Result<Vec<u8>, ContentError> {
        let at = self.elsewhere.get(path).unwrap_or(path);
        let bytes = self.source.read(at, row.size)?;
        let size = u64::try_from(bytes.len()).expect("a file length fits u64");
        if size != row.size || ContentHash::of(&bytes) != row.sha256 {
            return Err(ContentError::Changed { path: at.clone() });
        }
        Ok(bytes)
    }
}
