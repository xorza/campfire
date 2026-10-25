use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

use tempfile::TempDir;

use crate::dir_entries::{DirEntries, EntryKind};
use crate::durable_dir::DurableDir;
use crate::durable_file::DurableFile;
use crate::input_file::InputFile;
use campfire_platform::internals::FileLink;
use campfire_platform::{Exposure, UserOnly};

/// The most bytes a test reads of a file it wrote: far past any fixture, short of what a test
/// should hold.
const MAX_LEN: usize = 1 << 30;

/// A test's files: a temporary directory that is its user's only, so a data directory opens in
/// it, removed when it drops. Each call takes a path within it, relative to its root or absolute
/// under it, as the code a test runs gives one, which it asserts, so no test reaches out of it;
/// and panics with the path on a failure, as a test wants.
#[derive(Debug)]
pub struct TestDir {
    root: PathBuf,
    /// Removes the directory and all it holds when dropped.
    _temp: TempDir,
}

impl TestDir {
    #[expect(
        clippy::new_without_default,
        reason = "a test directory is made on the disk, which `Default` does not promise"
    )]
    pub fn new() -> Self {
        let temp = TempDir::new().expect("a temporary directory is made");
        let root = temp.path().join("test");
        UserOnly::create_dir(&root).expect("the test directory is made");
        Self { root, _temp: temp }
    }

    /// The test directory itself.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The path of `path` in the test directory, which need not exist: one relative to its
    /// root, or one absolute under it.
    pub fn path(&self, path: impl AsRef<Path>) -> PathBuf {
        let path = path.as_ref();
        let relative = if path.is_absolute() {
            path.strip_prefix(&self.root).ok()
        } else {
            Some(path)
        };
        let within = relative.filter(|relative| {
            relative
                .components()
                .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
        });
        let Some(relative) = within else {
            panic!("{} leaves the test directory", path.display());
        };
        self.root.join(relative)
    }

    /// Writes the file `relative`, durably and its user's only, in place of one there, and
    /// makes each directory above it that is missing.
    pub fn write(&self, relative: impl AsRef<Path>, bytes: impl AsRef<[u8]>) {
        let path = self.path(relative);
        let parent = path.parent().expect("a test file has a parent");
        DurableDir::create_all(parent).unwrap_or_else(|error| panic!("{error:?}"));
        DurableFile::write(&path, bytes.as_ref()).unwrap_or_else(|error| panic!("{error:?}"));
    }

    /// Adds `bytes` at the end of the file `relative`, in place: the file a handle holds open
    /// changes under it, as another program's write would, where `write` gives the name a new
    /// file.
    pub fn append(&self, relative: impl AsRef<Path>, bytes: impl AsRef<[u8]>) {
        let path = self.path(relative);
        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .and_then(|mut file| file.write_all(bytes.as_ref()))
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    }

    pub fn read(&self, relative: impl AsRef<Path>) -> Vec<u8> {
        InputFile::read(&self.path(relative), MAX_LEN).unwrap_or_else(|error| panic!("{error:?}"))
    }

    pub fn read_text(&self, relative: impl AsRef<Path>) -> String {
        InputFile::read_text(&self.path(relative), MAX_LEN)
            .unwrap_or_else(|error| panic!("{error:?}"))
    }

    /// The names of the entries of the directory `relative`, sorted.
    pub fn names(&self, relative: impl AsRef<Path>) -> Vec<String> {
        DirEntries::read(&self.path(relative))
            .unwrap_or_else(|error| panic!("{error:?}"))
            .into_iter()
            .map(|entry| entry.name.into_string().expect("a test file name is UTF-8"))
            .collect()
    }

    /// The kind of the entry that holds the name `relative`, a link as itself; none when no
    /// entry does.
    pub fn kind(&self, relative: impl AsRef<Path>) -> Option<EntryKind> {
        let path = self.path(relative);
        match fs::symlink_metadata(&path) {
            Ok(metadata) => Some(EntryKind::of(metadata.file_type())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => panic!("{}: {error:?}", path.display()),
        }
    }

    /// Whether an entry holds the name `relative`, a link included, whatever it names.
    pub fn exists(&self, relative: impl AsRef<Path>) -> bool {
        self.kind(relative).is_some()
    }

    /// Removes the file or the directory `relative`, a directory with all it holds.
    pub fn remove(&self, relative: impl AsRef<Path>) {
        let path = self.path(relative);
        let removed = match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(&path),
            _ => fs::remove_file(&path),
        };
        removed.unwrap_or_else(|error| panic!("{}: {error:?}", path.display()));
    }

    /// Gives the file or the directory `from` the name `to`, in place of a file there.
    pub fn rename(&self, from: impl AsRef<Path>, to: impl AsRef<Path>) {
        let (from, to) = (self.path(from), self.path(to));
        fs::rename(&from, &to).unwrap_or_else(|error| panic!("{}: {error:?}", from.display()));
    }

    /// Makes the directory `relative` and each one above it that is missing, durably and its
    /// user's only.
    pub fn create_dir(&self, relative: impl AsRef<Path>) {
        let path = self.path(relative);
        DurableDir::create_all(&path).unwrap_or_else(|error| panic!("{error:?}"));
    }

    /// Makes `link` a symbolic link to the file `target`, both relative to the root.
    pub fn link(&self, target: impl AsRef<Path>, link: impl AsRef<Path>) {
        FileLink::make(&self.path(target), &self.path(link));
    }

    /// Makes `link` a symbolic link to the directory `target`, both relative to the root.
    pub fn link_dir(&self, target: impl AsRef<Path>, link: impl AsRef<Path>) {
        FileLink::make_dir(&self.path(target), &self.path(link));
    }

    /// Lets other users read the file or the directory `relative`.
    pub fn expose(&self, relative: impl AsRef<Path>) {
        UserOnly::expose(&self.path(relative));
    }

    /// Who else may open the file or the directory `relative`; none when only its user may.
    pub fn exposure(&self, relative: impl AsRef<Path>) -> Option<Exposure> {
        let path = self.path(relative);
        UserOnly::exposure_at(&path).unwrap_or_else(|error| panic!("{}: {error:?}", path.display()))
    }
}

#[cfg(test)]
mod tests;
