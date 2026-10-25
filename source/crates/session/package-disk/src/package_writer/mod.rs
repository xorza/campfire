use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use campfire_common::PackageFingerprint;
use campfire_package_format::{ContentError, ContentHash, FileIndex, PackageWalk};
use campfire_store::{DurableDir, DurableFile, OutputFile};
use campfire_values::PackagePath;

use crate::error::WriteError;

/// A new package as its builder writes it, an importer or a packer: each file into a new
/// directory, written once with no sync, as the directories on its way are made with none, and
/// its row kept, then the index of the rows, synced, so
/// every file is checked against what was written whenever it is read, and a write a crash cut
/// short is found then. A file whose SHA-256 an earlier file has is its row alone, as the package
/// holds each SHA-256 once (05-protocol-spec.md, A package holds each SHA-256 once).
#[derive(Debug)]
pub struct PackageWriter {
    root: PathBuf,
    rows: PackageWalk,
    /// The SHA-256 of each file written.
    written: BTreeSet<ContentHash>,
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
            written: BTreeSet::new(),
            dirs: BTreeSet::new(),
        })
    }

    /// Takes the file at `path` with `bytes`: writes it, and the directories on its way, unless
    /// a file of the same bytes was written before, whose row then names its bytes; an error for
    /// a path taken before. The row is kept once the file is written, so a write refused leaves
    /// none.
    pub fn write(&mut self, path: PackagePath, bytes: &[u8]) -> Result<(), WriteError> {
        if self.rows.contains(&path) {
            return Err(WriteError::Package(ContentError::Repeated(path)));
        }
        let hashed = self.rows.hash(bytes).expect("a slice reads");
        if !self.written.contains(&hashed.sha256()) {
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
            OutputFile::write_new(&at, bytes).map_err(WriteError::File)?;
            self.written.insert(hashed.sha256());
        }
        self.rows.insert(path, hashed).map_err(WriteError::Package)
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
