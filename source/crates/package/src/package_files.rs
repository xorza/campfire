use std::collections::BTreeMap;
use std::io;

use campfire_content::{Fingerprint, PackagePath};
use serde::Serialize;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

use crate::error::ContentError;

/// A package's files, read once into memory: each file's bytes by its path in the package, and
/// the fingerprint over those bytes. A load parses only these bytes, so what it parses is what
/// the fingerprint names.
#[derive(Debug, Clone)]
pub struct PackageFiles {
    /// In the order of their paths, in bytes, as the fingerprint lists them.
    files: BTreeMap<PackagePath, Vec<u8>>,
    fingerprint: Fingerprint,
}

/// One row of a package's file list, as its fingerprint hashes it.
#[derive(Debug, Serialize)]
struct FileRow<'a> {
    path: &'a str,
    size: u64,
    sha256: [u8; 32],
}

impl PackageFiles {
    /// The package of `files`: the SHA-256 of the postcard list of each file's path, size and
    /// SHA-256, in the order of their paths.
    pub(crate) fn new(files: BTreeMap<PackagePath, Vec<u8>>) -> PackageFiles {
        let rows: Vec<FileRow<'_>> = files
            .iter()
            .map(|(path, bytes)| FileRow {
                path: path.as_str(),
                size: u64::try_from(bytes.len()).expect("a file length fits u64"),
                sha256: Sha256::digest(bytes).into(),
            })
            .collect();
        let list = postcard::to_allocvec(&rows).expect("a file list always encodes");
        let fingerprint = Fingerprint::new(Sha256::digest(list).into());
        PackageFiles { files, fingerprint }
    }

    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }

    /// The files under the directory `dir`, in the order of their paths.
    pub(crate) fn files_under<'a>(&'a self, dir: &'a str) -> impl Iterator<Item = &'a PackagePath> {
        self.files.keys().filter(move |path| path.is_under(dir))
    }

    /// The TOML data file at `path`, read as a `T`.
    pub fn read_data<T: DeserializeOwned>(&self, path: &PackagePath) -> Result<T, ContentError> {
        toml::from_str(self.read_text(path)?).map_err(|error| ContentError::Data {
            path: path.clone(),
            error,
        })
    }

    /// The text of the file at `path`, such as a script's source.
    pub fn read_text(&self, path: &PackagePath) -> Result<&str, ContentError> {
        let io = |error| ContentError::Io {
            path: path.clone(),
            error,
        };
        let bytes = self
            .files
            .get(path)
            .ok_or_else(|| io(io::Error::from(io::ErrorKind::NotFound)))?;
        str::from_utf8(bytes).map_err(|error| io(io::Error::new(io::ErrorKind::InvalidData, error)))
    }
}
