use std::path::{Path, PathBuf};

use campfire_package_format::{
    ContentError, DependencyPath, EncodedIndex, FileIndex, PackageFiles, PackagePlace,
    PackageSource, PackageWalk,
};
use campfire_store::{DirEntries, EntryKind, InputFile, PathError, ReadError};
use campfire_values::PackagePath;

use crate::error::IndexError;
use crate::package_dir::listings::Listings;
use crate::read_bound::ReadBound;

mod listings;

/// A package's directory on disk, the source the core reads the package's files from, through
/// its index.
#[derive(Debug, Clone)]
pub struct PackageDir {
    root: PathBuf,
}

impl PackageDir {
    /// The package on disk at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The package's files, as the core reads and checks them: see `PackageFiles::read`.
    pub fn files(&self) -> Result<PackageFiles, ContentError> {
        PackagePlace::files(self)
    }

    /// The index of the files under the package's root but its index, as its builder writes it:
    /// each hashed as it streams past. A link or any other entry that is neither a file nor a
    /// directory fails, as does a path that is not UTF-8 or that a package path does not spell,
    /// the first by name in each directory, so the first flaw of a tree is the same on every OS.
    pub fn index_files(&self) -> Result<EncodedIndex, IndexError> {
        let mut walk = PackageWalk::new();
        self.walk(&self.root, &mut walk)?;
        walk.finish().map_err(IndexError::Package)
    }

    /// The bytes of the file at `path`, as many as `bound` takes; a file not there is missing.
    fn read_bytes(&self, path: &PackagePath, bound: ReadBound) -> Result<Vec<u8>, ContentError> {
        let file = self.file_path(path);
        let ranges = InputFile::ranges(&file).map_err(|error| match error.error {
            ReadError::Missing => ContentError::Missing { path: path.clone() },
            _ => ContentError::Read(Box::new(error)),
        })?;
        let len = bound.take(ranges.len(), path, &file)?;
        ranges.read_at(0, len).map_err(|error| match error.error {
            ReadError::Short { .. } => ContentError::Changed { path: path.clone() },
            _ => ContentError::Read(Box::new(error)),
        })
    }

    /// Where the file at `path` of the package is, its names joined as the OS joins them.
    fn file_path(&self, path: &PackagePath) -> PathBuf {
        path.as_str()
            .split('/')
            .fold(self.root.clone(), |at, name| at.join(name))
    }

    /// Takes every file under `dir` into `walk`, in the order of their names, but the index at
    /// the root.
    fn walk(&self, dir: &Path, walk: &mut PackageWalk) -> Result<(), IndexError> {
        for entry in DirEntries::read(dir).map_err(IndexError::Read)? {
            let path = dir.join(&entry.name);
            match entry.kind {
                EntryKind::Dir => self.walk(&path, walk)?,
                EntryKind::File => {
                    let package_path = self.package_path(&path)?;
                    if package_path.as_str() != FileIndex::PATH {
                        let file = InputFile::stream(&path).map_err(IndexError::Read)?;
                        let hashed = walk.hash(file).map_err(|error| {
                            IndexError::Read(PathError {
                                path: path.clone(),
                                error: ReadError::Read(error),
                            })
                        })?;
                        walk.insert(package_path, hashed)
                            .map_err(IndexError::Package)?;
                    }
                }
                EntryKind::Link | EntryKind::Other => return Err(IndexError::NotAFile(path)),
            }
        }
        Ok(())
    }

    /// The package path of the file at `path`, under the root.
    fn package_path(&self, path: &Path) -> Result<PackagePath, IndexError> {
        let relative = path
            .strip_prefix(&self.root)
            .expect("a read path is under the root");
        let mut names = Vec::new();
        for component in relative.components() {
            let name = component.as_os_str().to_str();
            names.push(name.ok_or_else(|| IndexError::NotUtf8(path.to_owned()))?);
        }
        PackagePath::parse(&names.join("/")).ok_or_else(|| IndexError::NotPath(path.to_owned()))
    }
}

impl PackageSource for PackageDir {
    /// The bytes of the package's index, by its exact name, no more than an index holds.
    fn read_index(&self) -> Result<Vec<u8>, ContentError> {
        let path = PackagePath::parse(FileIndex::PATH).expect("the index's path is a package's");
        let held = Listings::default()
            .hold(&self.root, &path)
            .map_err(|error| ContentError::Read(Box::new(error)))?;
        if !held {
            return Err(ContentError::Missing { path });
        }
        self.read_bytes(&path, ReadBound::AtMost(FileIndex::MAX_LEN))
    }

    /// Each directory on the paths' way listed once.
    fn holds(
        &self,
        paths: &mut dyn Iterator<Item = &PackagePath>,
    ) -> Result<Vec<bool>, ContentError> {
        let mut listings = Listings::default();
        paths
            .map(|path| {
                listings
                    .hold(&self.root, path)
                    .map_err(|error| ContentError::Read(Box::new(error)))
            })
            .collect()
    }

    fn read(&self, path: &PackagePath, size: u64) -> Result<Vec<u8>, ContentError> {
        self.read_bytes(path, ReadBound::Exact(size))
    }
}

impl PackagePlace for PackageDir {
    /// The directory at `relative` from this one, which the OS resolves.
    fn join(&self, relative: &DependencyPath) -> Self {
        let root = relative
            .names()
            .fold(self.root.clone(), |root, name| root.join(name));
        Self { root }
    }

    fn dir(&self) -> &Path {
        &self.root
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::path::{Path, PathBuf};

    use crate::package_dir::PackageDir;

    impl PackageDir {
        /// `path` within the workspace's `packages` directory, where the tests and the checks
        /// find the test packages.
        pub fn workspace(path: &str) -> PathBuf {
            Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../packages")).join(path)
        }
    }
}

#[cfg(test)]
mod tests;
