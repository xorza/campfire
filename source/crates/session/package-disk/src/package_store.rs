use std::path::{Path, PathBuf};

use campfire_common::PackageFingerprint;
use campfire_package_format::{
    ContentError, FileIndex, PackageFiles, PackageFinder, PackageSource,
};
use campfire_store::{DirEntries, EntryKind, PathError, ReadError};

use crate::package_dir::PackageDir;

/// Packages by fingerprint: what a verifier holds, so a log's terms find the packages they name,
/// each found by its index alone and read once a log names it; and each package under its root
/// whose index does not read, with why, so one bad package costs only itself.
#[derive(Debug, Default)]
pub struct PackageStore {
    /// Sorted by fingerprint.
    packages: Vec<StoredPackage>,
    failures: Vec<StoreFailure>,
}

/// A package the store holds: its fingerprint, as its index gives it, and its directory.
#[derive(Debug)]
struct StoredPackage {
    fingerprint: PackageFingerprint,
    dir: PathBuf,
}

/// A package under a store's root that does not read: where, and why.
#[derive(Debug)]
pub struct StoreFailure {
    pub dir: PathBuf,
    pub error: ContentError,
}

impl PackageStore {
    /// Every package under `root`: each directory that holds a `manifest.toml`, which is not
    /// searched further; an error only when a directory that holds no package does not list.
    pub fn scan(root: &Path) -> Result<Self, PathError<ReadError>> {
        let mut store = Self::default();
        store.scan_dir(root)?;
        Ok(store)
    }

    /// Whether it holds a package of `fingerprint`.
    pub fn holds(&self, fingerprint: PackageFingerprint) -> bool {
        self.dir(fingerprint).is_some()
    }

    fn dir(&self, fingerprint: PackageFingerprint) -> Option<&Path> {
        let at = self
            .packages
            .binary_search_by_key(&fingerprint, |package| package.fingerprint)
            .ok()?;
        Some(&self.packages[at].dir)
    }

    /// The packages under the root whose index does not read, in the order of their directories.
    pub fn failures(&self) -> &[StoreFailure] {
        &self.failures
    }

    /// Adds the package in `dir`, by the fingerprint its index gives, or why its index does not
    /// read.
    fn insert(&mut self, dir: &Path) {
        let index = PackageDir::new(dir)
            .read_index()
            .and_then(|bytes| FileIndex::decode(&bytes));
        let fingerprint = match index {
            Ok(index) => index.fingerprint(),
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
            .binary_search_by_key(&fingerprint, |package| package.fingerprint)
        {
            let dir = dir.to_owned();
            self.packages.insert(at, StoredPackage { fingerprint, dir });
        }
    }

    /// Finds the packages under `dir`, in the order of their names, so the same tree gives the
    /// same result on every OS.
    fn scan_dir(&mut self, dir: &Path) -> Result<(), PathError<ReadError>> {
        let mut subdirs = Vec::new();
        for entry in DirEntries::read(dir)? {
            // By its exact name, which a file system that ignores case would also find as
            // `Manifest.toml`.
            if entry.kind == EntryKind::File && entry.name == PackageFiles::MANIFEST {
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

impl PackageFinder for PackageStore {
    fn find(&self, fingerprint: PackageFingerprint) -> Option<Result<PackageFiles, ContentError>> {
        Some(PackageDir::new(self.dir(fingerprint)?).files())
    }
}
