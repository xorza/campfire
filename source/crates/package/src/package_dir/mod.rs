use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use campfire_content::Fingerprint;
use campfire_content::PackagePath;
use serde::Serialize;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

use crate::error::ContentError;

/// A package's files: on disk, as a workspace holds them before a package is built, or in
/// memory, as a test builds them. Both read, list and fingerprint the same bytes the same way.
#[derive(Debug, Clone)]
pub struct PackageDir {
    root: PathBuf,
    source: Source,
}

/// Where a package's files are.
#[derive(Debug, Clone)]
enum Source {
    Disk,
    /// A tree of files by path, which `root` is a directory of.
    Memory(Arc<BTreeMap<PathBuf, Vec<u8>>>),
}

impl PackageDir {
    /// The file every package has at its root.
    pub const MANIFEST: &str = "manifest.toml";

    /// The package on disk at `root`.
    pub fn new(root: impl Into<PathBuf>) -> PackageDir {
        PackageDir {
            root: root.into(),
            source: Source::Disk,
        }
    }

    /// The package at `root` of the tree `files`, whose keys are paths from the tree's root.
    pub fn in_memory(
        files: Arc<BTreeMap<PathBuf, Vec<u8>>>,
        root: impl Into<PathBuf>,
    ) -> PackageDir {
        PackageDir {
            root: normal(&root.into()),
            source: Source::Memory(files),
        }
    }

    /// The package at `relative` from this one's root, as a manifest names a dependency.
    #[must_use]
    pub(crate) fn join(&self, relative: &Path) -> PackageDir {
        let root = self.root.join(relative);
        let root = match self.source {
            Source::Disk => root,
            Source::Memory(_) => normal(&root),
        };
        PackageDir {
            root,
            source: self.source.clone(),
        }
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// The package's fingerprint, from every file under its root. A link or any other entry that
    /// is neither a file nor a directory fails, as does a path that is not UTF-8.
    pub fn fingerprint(&self) -> Result<Fingerprint, ContentError> {
        let mut files = Vec::new();
        self.walk(&self.root, &mut files)?;
        let mut rows = Vec::with_capacity(files.len());
        for file in files {
            let bytes = self.read(&file.path).map_err(|error| ContentError::Scan {
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
    pub(crate) fn files_under(&self, dir: &str) -> Result<Vec<PackagePath>, ContentError> {
        let root = self.root.join(dir);
        let exists = match &self.source {
            Source::Disk => root.is_dir(),
            Source::Memory(files) => files.keys().any(|path| path.starts_with(&root)),
        };
        if !exists {
            return Ok(Vec::new());
        }
        let mut files = Vec::new();
        self.walk(&root, &mut files)?;
        Ok(files
            .iter()
            .map(|file| {
                PackagePath::parse(&file.relative).expect("a walked path stays in the package")
            })
            .collect())
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
        let io = |error| ContentError::Io {
            path: path.clone(),
            error,
        };
        let bytes = self.read(&self.root.join(path.as_path())).map_err(io)?;
        String::from_utf8(bytes)
            .map_err(|error| io(io::Error::new(io::ErrorKind::InvalidData, error)))
    }

    /// The bytes of the file at `path`, from the root of the files.
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        match &self.source {
            Source::Disk => fs::read(path),
            Source::Memory(files) => files
                .get(path)
                .cloned()
                .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound)),
        }
    }

    /// Every file under `dir`, sorted by its path from the root in bytes.
    fn walk(&self, dir: &Path, files: &mut Vec<Listed>) -> Result<(), ContentError> {
        let start = files.len();
        self.walk_unsorted(dir, files)?;
        files[start..].sort_unstable_by(|a, b| a.relative.as_bytes().cmp(b.relative.as_bytes()));
        Ok(())
    }

    fn walk_unsorted(&self, dir: &Path, files: &mut Vec<Listed>) -> Result<(), ContentError> {
        if let Source::Memory(tree) = &self.source {
            for path in tree.keys().filter(|path| path.starts_with(dir)) {
                files.push(self.listed(path.clone())?);
            }
            return Ok(());
        }
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
            files.push(self.listed(path)?);
        }
        Ok(())
    }

    /// The file at `path`, under the root, as a listing names it.
    fn listed(&self, path: PathBuf) -> Result<Listed, ContentError> {
        let relative = path
            .strip_prefix(&self.root)
            .expect("a listed path is under the root");
        let mut names = Vec::new();
        for component in relative.components() {
            let name = component.as_os_str().to_str();
            names.push(name.ok_or_else(|| ContentError::NotUtf8(path.clone()))?);
        }
        Ok(Listed {
            relative: names.join("/"),
            path,
        })
    }
}

/// `path` with each `..` taking away the name before it, and each `.` gone, as a tree in memory
/// keys its files.
fn normal(path: &Path) -> PathBuf {
    let mut normal = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                normal.pop();
            }
            Component::CurDir => {}
            other => normal.push(other),
        }
    }
    normal
}

/// One row of a package's file list, as its fingerprint hashes it.
#[derive(Debug, Serialize)]
struct FileRow {
    /// Relative to the package root, with `/` separators.
    path: String,
    size: u64,
    sha256: [u8; 32],
}

/// A file found under a package's root: its path from the root, with `/` separators, and its path
/// in its source.
#[derive(Debug)]
struct Listed {
    relative: String,
    path: PathBuf,
}

#[cfg(test)]
mod tests;
