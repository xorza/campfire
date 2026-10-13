use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use tempfile::TempDir;

use crate::dir_entries::{DirEntries, EntryKind};
use crate::durable_file::DurableFile;
use crate::input_file::InputFile;
use crate::platform::file_link::FileLink;
use crate::platform::owner_only::{Exposure, OwnerOnly};

/// The most bytes a test reads of a file it wrote: far past any fixture, short of what a test
/// should hold.
const MAX_LEN: usize = 1 << 30;

/// A test's files: a temporary directory that is its owner's only, so a data directory opens in
/// it, removed when it drops. Each call takes a path within it, relative to its root or absolute
/// under it, as the code a test runs gives one, which it asserts, so no test reaches out of it;
/// and panics with the path on a failure, as a test wants.
#[derive(Debug)]
pub struct Scratch {
    root: PathBuf,
    /// Removes the directory and all it holds when dropped.
    _temp: TempDir,
}

impl Scratch {
    #[expect(
        clippy::new_without_default,
        reason = "a scratch directory is made on the disk, which `Default` does not promise"
    )]
    pub fn new() -> Scratch {
        Scratch::made(TempDir::new())
    }

    /// A scratch directory under `parent`, as a test keeps its files in its own target's.
    pub fn new_in(parent: impl AsRef<Path>) -> Scratch {
        Scratch::made(TempDir::new_in(parent))
    }

    fn made(temp: io::Result<TempDir>) -> Scratch {
        let temp = temp.expect("a temporary directory is made");
        let root = temp.path().join("scratch");
        OwnerOnly::create_dir(&root).expect("the scratch directory is made");
        Scratch { root, _temp: temp }
    }

    /// The scratch directory itself.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The path of `path` in the scratch directory, which need not exist: one relative to its
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
            panic!("{} leaves the scratch directory", path.display());
        };
        self.root.join(relative)
    }

    /// Writes the file `relative`, durably and its owner's only, in place of one there, and
    /// makes each directory above it that is missing.
    pub fn write(&self, relative: impl AsRef<Path>, bytes: impl AsRef<[u8]>) {
        let path = self.path(relative);
        let parent = path.parent().expect("a scratch file has a parent");
        OwnerOnly::create_dir_all(parent)
            .unwrap_or_else(|error| panic!("{}: {error:?}", parent.display()));
        DurableFile::write(&path, bytes.as_ref()).unwrap_or_else(|error| panic!("{error:?}"));
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
            .map(|entry| entry.name.into_string().expect("a scratch name is UTF-8"))
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

    /// Makes the directory `relative` and each one above it that is missing, its owner's only.
    pub fn create_dir(&self, relative: impl AsRef<Path>) {
        let path = self.path(relative);
        OwnerOnly::create_dir_all(&path)
            .unwrap_or_else(|error| panic!("{}: {error:?}", path.display()));
    }

    /// Makes `link` a symbolic link to the file `target`, both relative to the root.
    pub fn link(&self, target: impl AsRef<Path>, link: impl AsRef<Path>) {
        FileLink::make(&self.path(target), &self.path(link));
    }

    /// Lets other users read the file or the directory `relative`.
    pub fn expose(&self, relative: impl AsRef<Path>) {
        OwnerOnly::expose(&self.path(relative));
    }

    /// Who else may open the file or the directory `relative`; none when only its owner may.
    pub fn exposure(&self, relative: impl AsRef<Path>) -> Option<Exposure> {
        let path = self.path(relative);
        OwnerOnly::exposure_at(&path)
            .unwrap_or_else(|error| panic!("{}: {error:?}", path.display()))
    }
}

#[cfg(test)]
mod tests;
