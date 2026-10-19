use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::Path;

use crate::durable_dir::error::{DurableDirCreateError, DurableDirError};
use crate::path_error::PathError;
use crate::platform::durable_name::DurableName;
use crate::platform::user_only::UserOnly;

pub(crate) mod error;

/// A directory made or removed durably: its parent is synced after, so the change of its name
/// survives a crash. Each directory it makes is its user's only.
#[derive(Debug)]
pub struct DurableDir;

/// What starts the name a removal gives a directory before it removes what the directory holds,
/// so a crash leaves it whole or gone, as Kafka renames a log's directory before it deletes it.
const TOMBSTONE: &str = ".campfire-removed.";

impl DurableDir {
    /// Makes the directory `path` when it is missing; one already there stays as it is.
    pub fn create(path: &Path) -> Result<(), PathError<DurableDirError>> {
        Self::create_case(path).map_err(PathError::at(path))
    }

    fn create_case(path: &Path) -> Result<(), DurableDirError> {
        let parent = path.parent().ok_or(DurableDirError::NoParent)?;
        match UserOnly::create_dir(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists && path.is_dir() => {}
            Err(error) => return Err(DurableDirError::Create(error)),
        }
        DurableName::sync_dir(parent).map_err(DurableDirError::SyncParent)
    }

    /// Makes the directory `path` and each one above it that is missing; one already there stays
    /// as it is.
    pub fn create_all(path: &Path) -> Result<(), PathError<DurableDirError>> {
        let missing: Vec<&Path> = path
            .ancestors()
            .take_while(|dir| !dir.as_os_str().is_empty() && !dir.is_dir())
            .collect();
        for dir in missing.into_iter().rev() {
            Self::create(dir)?;
        }
        Ok(())
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
