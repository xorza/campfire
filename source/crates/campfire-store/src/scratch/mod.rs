use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use tempfile::TempDir;

use crate::dir_entries::DirEntries;
use crate::durable_file::DurableFile;
use crate::input_file::InputFile;
use crate::platform::file_link::FileLink;
use crate::platform::owner_only::{Exposure, OwnerOnly};

/// The most bytes a test reads of a file it wrote: far past any fixture, short of what a test
/// should hold.
const MAX_LEN: usize = 1 << 30;

/// A test's files: a temporary directory that is its owner's only, so a data directory opens in
/// it, removed when it drops. Each call takes a path relative to its root, which it asserts, so
/// no test reaches out of it, and panics with the path on a failure, as a test wants.
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
        let temp = TempDir::new().expect("a temporary directory is made");
        let root = temp.path().join("scratch");
        OwnerOnly::create_dir(&root).expect("the scratch directory is made");
        Scratch { root, _temp: temp }
    }

    /// The path of `relative` in the scratch directory, which need not exist.
    pub fn path(&self, relative: impl AsRef<Path>) -> PathBuf {
        let relative = relative.as_ref();
        assert!(
            relative
                .components()
                .all(|component| matches!(component, Component::Normal(_) | Component::CurDir)),
            "{} leaves the scratch directory",
            relative.display()
        );
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

    /// Whether an entry holds the name `relative`, a link included, whatever it names.
    pub fn exists(&self, relative: impl AsRef<Path>) -> bool {
        let path = self.path(relative);
        match fs::symlink_metadata(&path) {
            Ok(_) => true,
            Err(error) if error.kind() == io::ErrorKind::NotFound => false,
            Err(error) => panic!("{}: {error:?}", path.display()),
        }
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
