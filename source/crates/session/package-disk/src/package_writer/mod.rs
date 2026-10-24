use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use campfire_common::PackageFingerprint;
use campfire_package_format::ContentHash;
use campfire_package_format::{FileIndex, PackageWalk};
use campfire_store::{DurableDir, DurableFile, OutputFile};
use campfire_values::PackagePath;

use crate::error::WriteError;

/// A new package as its builder writes it, an importer or a packer: each file into a new
/// directory, written once with no sync, as the directories on its way are made with none, and
/// its row kept, then the index of the rows, synced, so
/// every file is checked against what was written whenever it is read, and a write a crash cut
/// short is found then. A file whose SHA-256 an earlier file has is a hard link to that file, so
/// the package keeps one copy of identical files on disk (05-protocol-spec.md, Packages).
#[derive(Debug)]
pub struct PackageWriter {
    root: PathBuf,
    rows: PackageWalk,
    /// The first file written of each SHA-256.
    firsts: BTreeMap<ContentHash, PathBuf>,
    /// The directories it made, which a file after needs no call to make again.
    dirs: BTreeSet<PathBuf>,
}

impl PackageWriter {
    /// A writer of the package in the new directory `root`; an error when one is there.
    pub fn create(root: &Path) -> Result<Self, WriteError> {
        DurableDir::create_new(root).map_err(WriteError::CreateDir)?;
        Ok(Self {
            root: root.to_owned(),
            rows: PackageWalk::new(),
            firsts: BTreeMap::new(),
            dirs: BTreeSet::new(),
        })
    }

    /// Writes the file at `path` with `bytes`, and the directories on its way, linked to the
    /// first file of the same bytes; an error for a path written before. The row and the link's
    /// target are kept once the file is written, so a write refused leaves neither.
    pub fn write(&mut self, path: PackagePath, bytes: &[u8]) -> Result<(), WriteError> {
        let at = path
            .as_str()
            .split('/')
            .fold(self.root.clone(), |at, name| at.join(name));
        if let Some(dir) = at.parent()
            && !self.dirs.contains(dir)
        {
            OutputFile::create_dirs(dir).map_err(WriteError::Directory)?;
            self.dirs.insert(dir.to_owned());
        }
        let hashed = self.rows.hash(bytes).expect("a slice reads");
        match self.firsts.get(&hashed.sha256()) {
            Some(first) => OutputFile::link_new(&at, first, bytes),
            None => OutputFile::write_new(&at, bytes),
        }
        .map_err(WriteError::File)?;
        self.rows
            .insert(path, hashed)
            .map_err(WriteError::Package)?;
        self.firsts.entry(hashed.sha256()).or_insert(at);
        Ok(())
    }

    /// Writes the package's index; its fingerprint.
    pub fn finish(self) -> Result<PackageFingerprint, WriteError> {
        let encoded = self.rows.finish().map_err(WriteError::Package)?;
        DurableFile::write(&self.root.join(FileIndex::PATH), &encoded.bytes)
            .map_err(WriteError::Index)?;
        Ok(encoded.index.fingerprint())
    }
}

#[cfg(test)]
mod tests;
