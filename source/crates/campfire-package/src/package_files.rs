use std::collections::BTreeMap;

use campfire_capabilities::PackagePath;
use campfire_common::Fingerprint;
use serde::de::DeserializeOwned;

use crate::error::ContentError;

/// A package's files a load reads, read once into memory, each file's bytes by its path in the
/// package, and the fingerprint over every file of the package. A load parses only these bytes,
/// so what it parses is what the fingerprint names.
#[derive(Debug, Clone)]
pub struct PackageFiles {
    /// In the order of their paths.
    files: BTreeMap<PackagePath, Vec<u8>>,
    fingerprint: Fingerprint,
}

impl PackageFiles {
    /// The package of `files`, the ones a load reads, whose files' list hashes to
    /// `fingerprint`; see `PackageWalk::finish`.
    pub(crate) const fn new(
        files: BTreeMap<PackagePath, Vec<u8>>,
        fingerprint: Fingerprint,
    ) -> PackageFiles {
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
        let bytes = self
            .files
            .get(path)
            .ok_or_else(|| ContentError::Missing { path: path.clone() })?;
        str::from_utf8(bytes).map_err(|error| ContentError::NotText {
            path: path.clone(),
            error,
        })
    }
}
