use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::Path;

use crate::durable_dir::error::{DurableDirCreateError, DurableDirError};
use crate::path_error::PathError;
use campfire_platform::DurableName;
use campfire_platform::UserOnly;

pub(crate) mod error;

/// A directory made or removed durably: its parent is synced after, so the change of its name
/// survives a crash. Each directory it makes is its user's only.
#[derive(Debug)]
pub struct DurableDir;

/// Whether a directory's make made it or found it there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Made {
    New,
    There,
}

/// What starts the name a removal gives a directory before it removes what the directory holds,
/// so a crash leaves it whole or gone, as Kafka renames a log's directory before it deletes it.
const TOMBSTONE: &str = ".campfire-removed.";

impl DurableDir {
    /// Makes the directory `path` when it is missing, and syncs its parent so the new name
    /// survives a crash; one already there stays as it is, with no sync, and a link that holds the
    /// name is `Link`, not followed.
    pub fn create(path: &Path) -> Result<(), PathError<DurableDirError>> {
        Self::create_case(path).map_err(PathError::at(path))
    }

    fn create_case(path: &Path) -> Result<(), DurableDirError> {
        let parent = path.parent().ok_or(DurableDirError::NoParent)?;
        if Self::make(path)? == Made::New {
            DurableName::sync_dir(parent).map_err(DurableDirError::SyncParent)?;
        }
        Ok(())
    }

    /// Makes the directory `path`, its user's only, when it is missing, with no sync; a link
    /// that holds the name is `Link`, not followed.
    fn make(path: &Path) -> Result<Made, DurableDirError> {
        match UserOnly::create_dir(path) {
            Ok(()) => Ok(Made::New),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                match fs::symlink_metadata(path) {
                    Ok(named) if named.is_symlink() => Err(DurableDirError::Link),
                    Ok(named) if named.is_dir() => Ok(Made::There),
                    _ => Err(DurableDirError::Create(error)),
                }
            }
            Err(error) => Err(DurableDirError::Create(error)),
        }
    }

    /// Makes the directory `path` and each one above it that is missing, as `create` makes
    /// each; one already there stays as it is.
    pub fn create_all(path: &Path) -> Result<(), PathError<DurableDirError>> {
        Self::make_all(path, &mut Self::create_case)
    }

    /// Makes `path` by `make`, and first, when its parent is missing, the parent the same way:
    /// as std's `create_dir_all` tries the deepest first, so no directory there is asked whether
    /// it is, and one above that is a link, as a home is, is passed through, never made.
    pub(crate) fn make_all(
        path: &Path,
        make: &mut dyn FnMut(&Path) -> Result<(), DurableDirError>,
    ) -> Result<(), PathError<DurableDirError>> {
        match make(path) {
            Err(DurableDirError::Create(error)) if error.kind() == io::ErrorKind::NotFound => {
                let parent = path
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .ok_or(DurableDirError::NoParent)
                    .map_err(PathError::at(path))?;
                Self::make_all(parent, make)?;
                make(path).map_err(PathError::at(path))
            }
            made => made.map_err(PathError::at(path)),
        }
    }

    /// Makes the directory `path` and each one above it that is missing with no sync, for files
    /// written with none, as `OutputFile::write_new` writes them: a crash may lose them, and the
    /// read that checks the files finds it.
    pub(crate) fn make_all_unsynced(path: &Path) -> Result<(), PathError<DurableDirError>> {
        Self::make_all(path, &mut |dir| Self::make(dir).map(|_| ()))
    }

    /// Makes the directory `path` only when nothing holds its name: `Exists` otherwise, as a
    /// run's directory is its own.
    pub fn create_new(path: &Path) -> Result<(), PathError<DurableDirCreateError>> {
        let create = || {
            let parent = path.parent().ok_or(DurableDirError::NoParent)?;
            match UserOnly::create_dir(path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    return Err(DurableDirCreateError::Exists);
                }
                Err(error) => return Err(DurableDirError::Create(error).into()),
            }
            DurableName::sync_dir(parent).map_err(DurableDirError::SyncParent)?;
            Ok(())
        };
        create().map_err(PathError::at(path))
    }

    /// Gives the directory `from` the name `to`, in the same parent, where nothing holds `to`, so
    /// a directory made under one name takes its place whole.
    pub fn rename(from: &Path, to: &Path) -> Result<(), PathError<DurableDirError>> {
        let rename = || {
            let parent = to.parent().ok_or(DurableDirError::NoParent)?;
            DurableName::rename(from, to).map_err(DurableDirError::Rename)?;
            DurableName::sync_dir(parent).map_err(DurableDirError::SyncParent)
        };
        rename().map_err(PathError::at(to))
    }

    /// Removes the directory `path` and all it holds, at once as a crash sees it: it takes a
    /// tombstone's name, which no listing shows, then goes. The tombstones a crash left in its
    /// parent go first.
    pub fn remove(path: &Path) -> Result<(), PathError<DurableDirError>> {
        let remove = || {
            let parent = path.parent().ok_or(DurableDirError::NoParent)?;
            let name = path.file_name().ok_or(DurableDirError::NoParent)?;
            Self::sweep(parent)?;
            let mut tombstone = OsString::from(TOMBSTONE);
            tombstone.push(name);
            let tombstone = parent.join(tombstone);
            DurableName::rename(path, &tombstone).map_err(DurableDirError::Remove)?;
            DurableName::sync_dir(parent).map_err(DurableDirError::SyncParent)?;
            fs::remove_dir_all(&tombstone).map_err(DurableDirError::Remove)
        };
        remove().map_err(PathError::at(path))
    }

    /// Whether `name` is a tombstone's, which a listing leaves out.
    pub(crate) fn is_tombstone(name: &OsStr) -> bool {
        name.as_encoded_bytes().starts_with(TOMBSTONE.as_bytes())
    }

    /// Removes each tombstone in `parent`.
    fn sweep(parent: &Path) -> Result<(), DurableDirError> {
        for entry in fs::read_dir(parent).map_err(DurableDirError::Remove)? {
            let entry = entry.map_err(DurableDirError::Remove)?;
            if Self::is_tombstone(&entry.file_name()) {
                fs::remove_dir_all(entry.path()).map_err(DurableDirError::Remove)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
