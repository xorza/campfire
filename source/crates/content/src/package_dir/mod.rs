use std::fs;
use std::path::{Path, PathBuf};

use campfire_protocol::Fingerprint;
use serde::Serialize;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

use crate::error::ContentError;
use crate::package_path::PackagePath;

/// A package's files on disk, as the workspace holds them before a package is built.
#[derive(Debug, Clone)]
pub struct PackageDir {
    root: PathBuf,
}

impl PackageDir {
    /// The file every package has at its root.
    pub const MANIFEST: &str = "manifest.toml";

    pub fn new(root: impl Into<PathBuf>) -> PackageDir {
        PackageDir { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The package's fingerprint, from every file under its root. A link or any other entry that
    /// is neither a file nor a directory fails, as does a path that is not UTF-8.
    pub fn fingerprint(&self) -> Result<Fingerprint, ContentError> {
        let mut files = Vec::new();
        self.walk(&self.root, &mut files)?;
        let mut rows = Vec::with_capacity(files.len());
        for file in files {
            let bytes = fs::read(&file.path).map_err(|error| ContentError::Scan {
                dir: file.path.clone(),
                error,
            })?;
            rows.push(FileRow {
                path: file.relative,
                size: u64::try_from(bytes.len()).expect("a file length fits u64"),
                sha256: Sha256::digest(&bytes).into(),
            });
        }
        debug_assert!(rows.is_sorted_by(|a, b| a.path.as_bytes() < b.path.as_bytes()));
        let list = postcard::to_allocvec(&rows).expect("a file list always encodes");
        Ok(Fingerprint::new(Sha256::digest(list).into()))
    }

    /// The files under the directory `dir` of the package, sorted by path; none when it has no
    /// such directory.
    pub fn files_under(&self, dir: &str) -> Result<Vec<PackagePath>, ContentError> {
        let root = self.root.join(dir);
        if !root.is_dir() {
            return Ok(Vec::new());
        }
        let mut files = Vec::new();
        self.walk(&root, &mut files)?;
        files
            .iter()
            .map(|file| PackagePath::parse(&file.relative))
            .collect()
    }

    /// The TOML data file at `path`, read as a `T`.
    pub fn read_data<T: DeserializeOwned>(&self, path: &PackagePath) -> Result<T, ContentError> {
        toml::from_str(&self.read_text(path)?).map_err(|error| ContentError::Data {
            path: path.clone(),
            error,
        })
    }

    /// The text of the file at `path`, such as a script's source.
    pub fn read_text(&self, path: &PackagePath) -> Result<String, ContentError> {
        fs::read_to_string(self.root.join(path.as_path())).map_err(|error| ContentError::Io {
            path: path.clone(),
            error,
        })
    }

    /// Every file under `dir`, sorted by its path from the root in bytes.
    fn walk(&self, dir: &Path, files: &mut Vec<Listed>) -> Result<(), ContentError> {
        let start = files.len();
        self.walk_unsorted(dir, files)?;
        files[start..].sort_unstable_by(|a, b| a.relative.as_bytes().cmp(b.relative.as_bytes()));
        Ok(())
    }

    fn walk_unsorted(&self, dir: &Path, files: &mut Vec<Listed>) -> Result<(), ContentError> {
        let io = |error| ContentError::Scan {
            dir: dir.to_owned(),
            error,
        };
        for entry in fs::read_dir(dir).map_err(io)? {
            let entry = entry.map_err(io)?;
            let path = entry.path();
            let kind = entry.file_type().map_err(io)?;
            if kind.is_dir() {
                self.walk_unsorted(&path, files)?;
                continue;
            }
            if !kind.is_file() {
                return Err(ContentError::NotAFile(path));
            }
            let relative = path
                .strip_prefix(&self.root)
                .expect("a listed path is under the root");
            let mut names = Vec::new();
            for component in relative.components() {
                let name = component.as_os_str().to_str();
                names.push(name.ok_or_else(|| ContentError::NotUtf8(path.clone()))?);
            }
            files.push(Listed {
                relative: names.join("/"),
                path,
            });
        }
        Ok(())
    }
}

/// One row of a package's file list, as its fingerprint hashes it.
#[derive(Debug, Serialize)]
struct FileRow {
    /// Relative to the package root, with `/` separators.
    path: String,
    size: u64,
    sha256: [u8; 32],
}

/// A file found under a package's root: its path from the root, with `/` separators, and on disk.
#[derive(Debug)]
struct Listed {
    relative: String,
    path: PathBuf,
}

#[cfg(test)]
mod tests;
