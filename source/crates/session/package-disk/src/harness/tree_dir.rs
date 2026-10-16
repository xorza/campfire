use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use campfire_common::MapName;
use campfire_package::{
    ContentError, DependencyPath, EncodedIndex, FileIndex, LoadError, ModePackages, PackageFiles,
    PackageRef, PackageSource, PackageWalk,
};
use campfire_values::PackagePath;

use crate::error::IndexError;
use crate::package_place::{PackagePlace, ReadBound};

/// A package at `root` of a tree of files in memory, keyed by their paths from the tree's root,
/// as a test builds one: the source a load reads in place of a directory on disk. Its keys are
/// the files' exact names.
#[derive(Debug, Clone)]
pub struct TreeDir {
    tree: Arc<BTreeMap<PathBuf, Vec<u8>>>,
    root: PathBuf,
}

impl TreeDir {
    /// The package at `root` of `tree`.
    pub fn new(tree: Arc<BTreeMap<PathBuf, Vec<u8>>>, root: impl AsRef<Path>) -> Self {
        Self {
            tree,
            root: normal(root.as_ref()),
        }
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
                let index = Self::new(Arc::clone(&tree), root).index_files().unwrap();
                (root.join(FileIndex::PATH), index.bytes)
            })
            .collect();
        let mut files = Arc::into_inner(tree).expect("no package holds the tree");
        files.extend(indexes);
        files
    }

    /// The package's files, as the core reads and checks them: see `PackageFiles::read`.
    pub fn files(&self) -> Result<PackageFiles, ContentError> {
        PackagePlace::files(self)
    }

    /// The mode at this root on its map `map`, or its only map when `map` names none, with its
    /// dependencies at the paths its manifest gives from it.
    pub fn mode_packages(&self, map: Option<&MapName>) -> Result<ModePackages, LoadError> {
        PackagePlace::mode_packages(self, map)
    }

    /// The index of the tree's files under the root but its index, in the order of their paths.
    pub fn index_files(&self) -> Result<EncodedIndex, IndexError> {
        let mut walk = PackageWalk::new();
        for (path, bytes) in self
            .tree
            .iter()
            .filter(|(path, _)| path.starts_with(&self.root))
        {
            let relative = path
                .strip_prefix(&self.root)
                .expect("a file under the root");
            let names = relative
                .components()
                .map(|component| component.as_os_str().to_str())
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| IndexError::NotUtf8(path.clone()))?;
            let package_path = PackagePath::parse(&names.join("/"))
                .ok_or_else(|| IndexError::NotPath(path.clone()))?;
            if package_path.as_str() != FileIndex::PATH {
                let hashed = walk.hash(bytes.as_slice()).expect("a slice reads");
                walk.insert(package_path, hashed)
                    .map_err(IndexError::Package)?;
            }
        }
        walk.finish().map_err(IndexError::Package)
    }

    /// The bytes of the file at `path`, as many as `bound` takes; a file not there is missing.
    fn read_bytes(&self, path: &PackagePath, bound: ReadBound) -> Result<Vec<u8>, ContentError> {
        let file = path
            .as_str()
            .split('/')
            .fold(self.root.clone(), |at, name| at.join(name));
        let bytes = self
            .tree
            .get(&file)
            .ok_or_else(|| ContentError::Missing { path: path.clone() })?;
        bound.take(
            u64::try_from(bytes.len()).expect("a length fits u64"),
            path,
            &file,
        )?;
        Ok(bytes.clone())
    }
}

impl PackageSource for TreeDir {
    fn read_index(&self) -> Result<Vec<u8>, ContentError> {
        let path = PackagePath::parse(FileIndex::PATH).expect("the index's path is a package's");
        self.read_bytes(&path, ReadBound::AtMost(FileIndex::MAX_LEN))
    }

    /// Nothing to check: the tree keys each file by its exact name.
    fn check_names(&self, _: &mut dyn Iterator<Item = &PackagePath>) -> Result<(), ContentError> {
        Ok(())
    }

    fn read(&self, path: &PackagePath, size: u64) -> Result<Vec<u8>, ContentError> {
        self.read_bytes(path, ReadBound::Exact(size))
    }
}

impl PackagePlace for TreeDir {
    /// The directory at `relative` from this one, each `..` taking the name before it away, as
    /// the tree keys its files.
    fn join(&self, relative: &DependencyPath) -> Self {
        let root = relative
            .names()
            .fold(self.root.clone(), |root, name| root.join(name));
        Self::new(Arc::clone(&self.tree), root)
    }

    fn package_ref(&self) -> PackageRef {
        PackageRef::Dir(self.root.clone())
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
