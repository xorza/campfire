use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use campfire_common::MapName;
use campfire_package::{
    ContentError, FileIndex, LoadError, LoadProblem, ModePackages, PackageFiles, PackageRef,
    PackageSource, PackageWalk,
};
use campfire_store::{DirEntries, EntryKind, InputFile, ReadError};
use campfire_values::PackagePath;

use crate::error::IndexError;
use crate::package_dir::listings::Listings;

mod listings;

/// Where a package's files are: on disk, or in memory, as a test builds them. Both are the
/// source the core reads the package's files from, through its index.
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
    fn join(&self, relative: &Path) -> PackageDir {
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

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The package's files, as the core reads and checks them: see `PackageFiles::read`.
    pub fn files(&self) -> Result<PackageFiles, ContentError> {
        PackageFiles::read(Arc::new(self.clone()))
    }

    /// The map of the mode in this directory that a session plays, as
    /// `ModePackages::choose_map` chooses it from the mode's index.
    pub fn choose_map(&self, map: Option<MapName>) -> Result<MapName, LoadError> {
        let package = PackageRef::Dir(self.root.clone());
        if let Some(map) = map {
            return Ok(map);
        }
        let index = self
            .read_index()
            .and_then(|bytes| FileIndex::decode(&bytes))
            .map_err(|error| LoadError::new(package.clone(), LoadProblem::Content(error)))?;
        ModePackages::choose_map(&index, &package, None)
    }

    /// The mode in this directory with its map `map`, and its dependencies at the paths its
    /// manifest gives from it, each read once.
    pub fn mode_packages(&self, map: &MapName) -> Result<ModePackages, LoadError> {
        let read = |dir: &PackageDir, package: PackageRef| {
            dir.files()
                .map_err(|error| LoadError::new(package, LoadProblem::Content(error)))
        };
        let at = PackageRef::Dir(self.root.clone());
        let files = read(self, at.clone())?;
        let manifest = files.mode_manifest(&at)?;
        let dependencies = manifest
            .dependencies
            .iter()
            .map(|(name, dependency)| {
                let dir = self.join(Path::new(&dependency.path));
                Ok((name.clone(), read(&dir, PackageRef::Name(name.clone()))?))
            })
            .collect::<Result<Vec<_>, LoadError>>()?;
        let dependencies: Vec<_> = dependencies
            .iter()
            .map(|(name, files)| (name.clone(), files))
            .collect();
        ModePackages::new(&files, manifest, map, &dependencies)
    }

    /// The index of the files under the package's root but its index, as its builder writes it:
    /// each hashed as it streams past. A link or any other entry that is neither a file nor a
    /// directory fails, as does a path that is not UTF-8 or that a package path does not spell,
    /// the first by name in each directory, so the first flaw of a tree is the same on every OS.
    pub fn index_files(&self) -> Result<FileIndex, IndexError> {
        let mut walk = PackageWalk::new();
        match &self.source {
            Source::Disk => self.walk_disk(&self.root, &mut walk)?,
            Source::Memory(tree) => {
                for (path, bytes) in tree.iter().filter(|(path, _)| path.starts_with(&self.root)) {
                    let path = self.package_path(path)?;
                    if path.as_str() != FileIndex::PATH {
                        walk.add(path, bytes.as_slice())
                            .map_err(IndexError::Package)?;
                    }
                }
            }
        }
        walk.finish().map_err(IndexError::Package)
    }

    /// Missing, unless the file at `path` is on disk by its exact name, by the names `listings`
    /// holds of the directories on its way. A tree in memory keys its files by their exact
    /// paths.
    fn check_name(&self, path: &PackagePath, listings: &mut Listings) -> Result<(), ContentError> {
        let held = match self.source {
            Source::Disk => listings
                .hold(&self.root, path)
                .map_err(|error| ContentError::Read(Box::new(error)))?,
            Source::Memory(_) => true,
        };
        if held {
            Ok(())
        } else {
            Err(ContentError::Missing { path: path.clone() })
        }
    }

    /// The bytes of the file at `path`: `size` of them, a file of another size `Changed`, or as
    /// many as it holds as it opens; a file not there is missing.
    fn read_bytes(&self, path: &PackagePath, size: Option<u64>) -> Result<Vec<u8>, ContentError> {
        let missing = || ContentError::Missing { path: path.clone() };
        let changed = || ContentError::Changed { path: path.clone() };
        match &self.source {
            Source::Disk => {
                let mut ranges = InputFile::ranges(&self.file_path(path)).map_err(|error| {
                    match error.error {
                        ReadError::Missing => missing(),
                        _ => ContentError::Read(Box::new(error)),
                    }
                })?;
                let len = size.unwrap_or(ranges.len());
                if ranges.len() != len {
                    return Err(changed());
                }
                let len = usize::try_from(len).expect("a file's length fits usize");
                ranges.read_at(0, len).map_err(|error| match error.error {
                    ReadError::Short { .. } => changed(),
                    _ => ContentError::Read(Box::new(error)),
                })
            }
            Source::Memory(tree) => tree.get(&self.file_path(path)).cloned().ok_or_else(missing),
        }
    }

    /// Where the file at `path` of the package is, its names joined as the OS joins them.
    fn file_path(&self, path: &PackagePath) -> PathBuf {
        path.as_str()
            .split('/')
            .fold(self.root.clone(), |at, name| at.join(name))
    }

    /// Takes every file under `dir` on disk into `walk`, in the order of their names, but the
    /// index at the root.
    fn walk_disk(&self, dir: &Path, walk: &mut PackageWalk) -> Result<(), IndexError> {
        for entry in DirEntries::read(dir).map_err(IndexError::Read)? {
            let path = dir.join(&entry.name);
            match entry.kind {
                EntryKind::Dir => self.walk_disk(&path, walk)?,
                EntryKind::File => {
                    let package_path = self.package_path(&path)?;
                    if package_path.as_str() != FileIndex::PATH {
                        let file = InputFile::stream(&path).map_err(IndexError::Read)?;
                        walk.add(package_path, file).map_err(IndexError::Package)?;
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
    /// The bytes of the package's index, by its exact name, whose own size is its bound.
    fn read_index(&self) -> Result<Vec<u8>, ContentError> {
        let path = PackagePath::parse(FileIndex::PATH).expect("the index's path is a package's");
        self.check_name(&path, &mut Listings::default())?;
        self.read_bytes(&path, None)
    }

    /// Each directory on the paths' way listed once.
    fn check_names(
        &self,
        paths: &mut dyn Iterator<Item = &PackagePath>,
    ) -> Result<(), ContentError> {
        let mut listings = Listings::default();
        for path in paths {
            self.check_name(path, &mut listings)?;
        }
        Ok(())
    }

    fn read(&self, path: &PackagePath, size: u64) -> Result<Vec<u8>, ContentError> {
        self.read_bytes(path, Some(size))
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

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use campfire_package::{FileIndex, PackageFiles};

    use crate::package_dir::PackageDir;

    impl PackageDir {
        /// `path` within the workspace's `packages` directory, where the tests and the checks
        /// find the test packages.
        pub fn workspace(path: &str) -> PathBuf {
            Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../packages")).join(path)
        }

        /// `files` with the index of each package in it, each directory that holds a
        /// `manifest.toml`, written again from its files, as its builder would after an edit.
        pub fn reindexed(files: BTreeMap<PathBuf, Vec<u8>>) -> BTreeMap<PathBuf, Vec<u8>> {
            let tree = Arc::new(files);
            let indexes: Vec<(PathBuf, Vec<u8>)> = tree
                .keys()
                .filter(|path| path.file_name() == Some(PackageFiles::MANIFEST.as_ref()))
                .map(|path| {
                    let root = path.parent().expect("a manifest is in a directory");
                    let index = PackageDir::in_memory(Arc::clone(&tree), root)
                        .index_files()
                        .unwrap();
                    (root.join(FileIndex::PATH), index.bytes().to_vec())
                })
                .collect();
            let mut files = Arc::into_inner(tree).expect("no package holds the tree");
            files.extend(indexes);
            files
        }
    }
}

#[cfg(test)]
mod tests;
