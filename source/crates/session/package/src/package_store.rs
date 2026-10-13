use std::path::{Path, PathBuf};

use campfire_common::Fingerprint;
use campfire_store::{DirEntries, EntryKind};

use crate::error::ContentError;
use crate::package_dir::PackageDir;
use crate::package_files::PackageFiles;

/// Packages by fingerprint: what a verifier holds, so a log's terms find the packages they name;
/// and each package under its root that does not read, with why, so one bad package costs only
/// itself.
#[derive(Debug, Default)]
pub struct PackageStore {
    /// Sorted by fingerprint.
    packages: Vec<PackageFiles>,
    failures: Vec<StoreFailure>,
}

/// A package under a store's root that does not read: where, and why.
#[derive(Debug)]
pub struct StoreFailure {
    pub dir: PathBuf,
    pub error: ContentError,
}

impl PackageStore {
    /// Every package under `root`: each directory that holds a `manifest.toml`, which is not
    /// searched further; an error only when a directory that holds no package does not read.
    pub fn scan(root: &Path) -> Result<PackageStore, ContentError> {
        let mut store = PackageStore::default();
        store.scan_dir(root)?;
        Ok(store)
    }

    pub fn get(&self, fingerprint: Fingerprint) -> Option<&PackageFiles> {
        let at = self
            .packages
            .binary_search_by_key(&fingerprint, PackageFiles::fingerprint)
            .ok()?;
        Some(&self.packages[at])
    }

    /// The packages under the root that do not read, in the order of their directories.
    pub fn failures(&self) -> &[StoreFailure] {
        &self.failures
    }

    /// Adds the package in `dir`, by its fingerprint, or why it does not read.
    fn insert(&mut self, dir: &Path) {
        let files = match PackageDir::new(dir).read() {
            Ok(files) => files,
            Err(error) => {
                self.failures.push(StoreFailure {
                    dir: dir.to_owned(),
                    error,
                });
                return;
            }
        };
        if let Err(at) = self
            .packages
            .binary_search_by_key(&files.fingerprint(), PackageFiles::fingerprint)
        {
            self.packages.insert(at, files);
        }
    }

    /// Finds the packages under `dir`, in the order of their names, so the same tree gives the
    /// same result on every OS.
    fn scan_dir(&mut self, dir: &Path) -> Result<(), ContentError> {
        let mut subdirs = Vec::new();
        for entry in DirEntries::read(dir).map_err(ContentError::Read)? {
            // By its exact name, which a file system that ignores case would also find as
            // `Manifest.toml`.
            if entry.kind == EntryKind::File && entry.name == PackageDir::MANIFEST {
                self.insert(dir);
                return Ok(());
            }
            if entry.kind == EntryKind::Dir {
                subdirs.push(dir.join(&entry.name));
            }
        }
        for subdir in subdirs {
            self.scan_dir(&subdir)?;
        }
        Ok(())
    }
}
