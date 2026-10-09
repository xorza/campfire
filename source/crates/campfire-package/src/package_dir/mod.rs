use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use campfire_capabilities::PackagePath;
use campfire_store::{DirEntries, EntryKind, InputFile, ReadError};
use sha2::{Digest, Sha256};

use crate::error::ContentError;
use crate::file_index::{FileIndex, FileRow};
use crate::package::SCRIPTS;
use crate::package_files::PackageFiles;
use crate::package_text::LOCALE;
use crate::package_walk::PackageWalk;

/// Where a package's files are: on disk, or in memory, as a test builds them. Both read through
/// the package's index into the same `PackageFiles`.
#[derive(Debug, Clone)]
pub struct PackageDir {
    root: PathBuf,
    source: Source,
}

/// The directories whose files a load reads, beside the manifest.
const READ_DIRS: [&str; 4] = ["data", "map", SCRIPTS, LOCALE];

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

    /// The package's index, and the files a load reads, the manifest's and those under `data/`,
    /// `map/`, `scripts/` and `locale/`, each read once into memory and checked against its row.
    /// No other file is read, nor any file the index does not list.
    pub fn read(&self) -> Result<PackageFiles, ContentError> {
        let index = FileIndex::decode(&self.read_index()?)?;
        let mut files = BTreeMap::new();
        for (path, row) in index.rows() {
            if PackageDir::reads(path) {
                files.insert(path.clone(), self.read_row(path, row)?);
            }
        }
        Ok(PackageFiles::new(files, index, self.clone()))
    }

    /// The index of the files under the package's root but its index, as its builder writes it:
    /// each hashed as it streams past. A link or any other entry that is neither a file nor a
    /// directory fails, as does a path that is not UTF-8 or that a package path does not spell,
    /// the first by name in each directory, so the first flaw of a tree is the same on every OS.
    pub fn index_files(&self) -> Result<FileIndex, ContentError> {
        let mut walk = PackageWalk::new();
        match &self.source {
            Source::Disk => self.walk_disk(&self.root, &mut walk)?,
            Source::Memory(tree) => {
                for (path, bytes) in tree.iter().filter(|(path, _)| path.starts_with(&self.root)) {
                    let path = self.package_path(path)?;
                    if path.as_str() != FileIndex::PATH {
                        walk.add(path, bytes.as_slice())?;
                    }
                }
            }
        }
        walk.finish()
    }

    /// The bytes of the file at `path`, whose row is `row`, read no further than its row's size
    /// and refused when they differ from it.
    pub(crate) fn read_row(
        &self,
        path: &PackagePath,
        row: FileRow,
    ) -> Result<Vec<u8>, ContentError> {
        let bytes = self.read_bytes(path, Some(row.size))?;
        let size = u64::try_from(bytes.len()).expect("a file length fits u64");
        if size != row.size || <[u8; 32]>::from(Sha256::digest(&bytes)) != row.sha256 {
            return Err(ContentError::Changed { path: path.clone() });
        }
        Ok(bytes)
    }

    /// The bytes of the package's index, whose own size is its bound.
    fn read_index(&self) -> Result<Vec<u8>, ContentError> {
        self.read_bytes(&PackageDir::engine_path(FileIndex::PATH), None)
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
                        _ => ContentError::Read(error),
                    }
                })?;
                let len = size.unwrap_or(ranges.len());
                if ranges.len() != len {
                    return Err(changed());
                }
                let len = usize::try_from(len).expect("a file's length fits usize");
                ranges.read_at(0, len).map_err(|error| match error.error {
                    ReadError::Short { .. } => changed(),
                    _ => ContentError::Read(error),
                })
            }
            Source::Memory(tree) => tree.get(&self.file_path(path)).cloned().ok_or_else(missing),
        }
    }

    /// Whether a load reads the file at `path`: the manifest, and the files of its data, its
    /// map, its scripts and its locales.
    fn reads(path: &PackagePath) -> bool {
        path.as_str() == PackageDir::MANIFEST || READ_DIRS.iter().any(|dir| path.is_under(dir))
    }

    /// Where the file at `path` of the package is, its names joined as the OS joins them.
    fn file_path(&self, path: &PackagePath) -> PathBuf {
        path.as_str()
            .split('/')
            .fold(self.root.clone(), |at, name| at.join(name))
    }

    /// Takes every file under `dir` on disk into `walk`, in the order of their names, but the
    /// index at the root.
    fn walk_disk(&self, dir: &Path, walk: &mut PackageWalk) -> Result<(), ContentError> {
        for entry in DirEntries::read(dir).map_err(ContentError::Read)? {
            let path = dir.join(&entry.name);
            match entry.kind {
                EntryKind::Dir => self.walk_disk(&path, walk)?,
                EntryKind::File => {
                    let package_path = self.package_path(&path)?;
                    if package_path.as_str() != FileIndex::PATH {
                        let file = InputFile::stream(&path).map_err(ContentError::Read)?;
                        walk.add(package_path, file)?;
                    }
                }
                EntryKind::Link | EntryKind::Other => return Err(ContentError::NotAFile(path)),
            }
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
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use campfire_store::{DirEntries, EntryKind, InputFile};

    use crate::file_index::FileIndex;
    use crate::package_dir::PackageDir;

    /// The most bytes a test reads of a workspace package's file: far past any the test
    /// packages hold.
    const TREE_FILE_LEN: usize = 64 << 20;

    impl PackageDir {
        /// `path` within the workspace's `packages` directory, where the tests and the checks
        /// find the test packages.
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

        /// `files` with the index of each package in it, each directory that holds a
        /// `manifest.toml`, written again from its files, as its builder would after an edit.
        pub fn reindexed(files: BTreeMap<PathBuf, Vec<u8>>) -> BTreeMap<PathBuf, Vec<u8>> {
            let roots: Vec<PathBuf> = files
                .keys()
                .filter(|path| path.file_name() == Some(PackageDir::MANIFEST.as_ref()))
                .map(|path| {
                    path.parent()
                        .expect("a manifest is in a directory")
                        .to_owned()
                })
                .collect();
            let tree = Arc::new(files);
            let indexes: Vec<(PathBuf, Vec<u8>)> = roots
                .into_iter()
                .map(|root| {
                    let index = PackageDir::in_memory(Arc::clone(&tree), &root)
                        .index_files()
                        .unwrap();
                    (root.join(FileIndex::PATH), index.bytes().to_vec())
                })
                .collect();
            let mut files = Arc::into_inner(tree).expect("no package holds the tree");
            files.extend(indexes);
            files
        }

        fn read_tree(dir: &Path, at: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
            for entry in DirEntries::read(dir).unwrap() {
                let (on_disk, path) = (dir.join(&entry.name), at.join(&entry.name));
                if entry.kind == EntryKind::Dir {
                    PackageDir::read_tree(&on_disk, &path, files);
                } else {
                    files.insert(path, InputFile::read(&on_disk, TREE_FILE_LEN).unwrap());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
