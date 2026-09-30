use std::fs;
use std::path::Path;

use crate::error::ContentError;
use crate::fingerprint::Fingerprint;

use crate::package_dir::PackageDir;

/// Packages by fingerprint: what a verifier holds, so a log's terms find the packages they name.
#[derive(Debug, Default)]
pub struct PackageStore {
    /// Sorted by fingerprint.
    packages: Vec<(Fingerprint, PackageDir)>,
}

impl PackageStore {
    /// Every package under `root`: each directory that holds a `manifest.toml`, which is not
    /// searched further.
    pub fn scan(root: &Path) -> Result<PackageStore, ContentError> {
        let mut store = PackageStore::default();
        store.scan_dir(root)?;
        Ok(store)
    }

    /// Adds the package in `dir`, by its fingerprint.
    fn insert(&mut self, dir: PackageDir) -> Result<(), ContentError> {
        let fingerprint = dir.fingerprint()?;
        if let Err(at) = self
            .packages
            .binary_search_by_key(&fingerprint, |(held, _)| *held)
        {
            self.packages.insert(at, (fingerprint, dir));
        }
        Ok(())
    }

    pub fn get(&self, fingerprint: Fingerprint) -> Option<&PackageDir> {
        let at = self
            .packages
            .binary_search_by_key(&fingerprint, |(held, _)| *held)
            .ok()?;
        Some(&self.packages[at].1)
    }

    fn scan_dir(&mut self, dir: &Path) -> Result<(), ContentError> {
        let io = |error| ContentError::Scan {
            dir: dir.to_owned(),
            error,
        };
        if dir.join(PackageDir::MANIFEST).is_file() {
            self.insert(PackageDir::new(dir))?;
            return Ok(());
        }
        let mut subdirs = Vec::new();
        for entry in fs::read_dir(dir).map_err(io)? {
            let entry = entry.map_err(io)?;
            if entry.file_type().map_err(io)?.is_dir() {
                subdirs.push(entry.path());
            }
        }
        subdirs.sort_unstable();
        for subdir in subdirs {
            self.scan_dir(&subdir)?;
        }
        Ok(())
    }
}
