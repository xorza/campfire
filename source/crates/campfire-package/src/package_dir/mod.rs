use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use campfire_capabilities::PackagePath;

use crate::error::ContentError;
use crate::package_files::PackageFiles;

/// Where a package's files are: on disk, as a workspace holds them before a package is built, or
/// in memory, as a test builds them. Both read into the same `PackageFiles`.
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

    /// One of the engine's paths in a package, as `PackageFiles` reads it.
    pub(crate) fn engine_path(path: &str) -> PackagePath {
        PackagePath::parse(path).expect("the engine's paths are in the package")
    }

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

    /// Every file under the package's root, read once into memory. A link or any other entry
    /// that is neither a file nor a directory fails, as does a path that is not UTF-8 or that a
    /// package path does not spell.
    pub fn read(&self) -> Result<PackageFiles, ContentError> {
        let mut files = BTreeMap::new();
        match &self.source {
            Source::Disk => self.read_disk(&self.root, &mut files)?,
            Source::Memory(tree) => {
                for (path, bytes) in tree.iter().filter(|(path, _)| path.starts_with(&self.root)) {
                    files.insert(self.package_path(path)?, bytes.clone());
                }
            }
        }
        Ok(PackageFiles::new(files))
    }

    /// Reads every file under `dir` on disk into `files`.
    fn read_disk(
        &self,
        dir: &Path,
        files: &mut BTreeMap<PackagePath, Vec<u8>>,
    ) -> Result<(), ContentError> {
        let io = |error| ContentError::Scan {
            dir: dir.to_owned(),
            error,
        };
        for entry in fs::read_dir(dir).map_err(io)? {
            let entry = entry.map_err(io)?;
            let path = entry.path();
            let kind = entry.file_type().map_err(io)?;
            if kind.is_dir() {
                self.read_disk(&path, files)?;
                continue;
            }
            if !kind.is_file() {
                return Err(ContentError::NotAFile(path));
            }
            let bytes = fs::read(&path).map_err(|error| ContentError::Scan {
                dir: path.clone(),
                error,
            })?;
            files.insert(self.package_path(&path)?, bytes);
        }
        Ok(())
    }

    /// The package path of the file at `path`, under the root.
    fn package_path(&self, path: &Path) -> Result<PackagePath, ContentError> {
        let relative = path
            .strip_prefix(&self.root)
            .expect("a read path is under the root");
        let mut names = Vec::new();
        for component in relative.components() {
            let name = component.as_os_str().to_str();
            names.push(name.ok_or_else(|| ContentError::NotUtf8(path.to_owned()))?);
        }
        PackagePath::parse(&names.join("/")).ok_or_else(|| ContentError::NotPath(path.to_owned()))
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

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};

    use crate::package_dir::PackageDir;

    impl PackageDir {
        /// `path` within the workspace's `packages` directory, where the tests and the checks
        /// find the test and reference packages.
        pub fn workspace(path: &str) -> PathBuf {
            Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../packages")).join(path)
        }

        /// Every file under `path` within the workspace's `packages` directory, by its path from
        /// there, read from disk: a tree for `PackageDir::in_memory`.
        pub fn workspace_tree(path: &str) -> BTreeMap<PathBuf, Vec<u8>> {
            let mut files = BTreeMap::new();
            PackageDir::read_tree(&PackageDir::workspace(path), Path::new(""), &mut files);
            files
        }

        fn read_tree(dir: &Path, at: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
            for entry in fs::read_dir(dir).unwrap() {
                let entry = entry.unwrap();
                let path = at.join(entry.file_name());
                if entry.file_type().unwrap().is_dir() {
                    PackageDir::read_tree(&entry.path(), &path, files);
                } else {
                    files.insert(path, fs::read(entry.path()).unwrap());
                }
            }
        }
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "a test makes and removes the files of its fixtures"
)]
#[cfg(test)]
mod tests;
