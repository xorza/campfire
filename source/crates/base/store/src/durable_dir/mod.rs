use std::fs;
use std::io;
use std::path::Path;

use crate::durable_dir::error::{DurableDirCreateError, DurableDirError};
use crate::path_error::PathError;
use crate::platform::durable_name::DurableName;
use crate::platform::owner_only::OwnerOnly;

pub(crate) mod error;

/// A directory made or removed durably: its parent is synced after, so the change of its name
/// survives a crash. Each directory it makes is its owner's only.
#[derive(Debug)]
pub struct DurableDir;

impl DurableDir {
    /// Makes the directory `path` when it is missing; one already there stays as it is.
    pub fn create(path: &Path) -> Result<(), PathError<DurableDirError>> {
        DurableDir::create_case(path).map_err(PathError::at(path))
    }

    fn create_case(path: &Path) -> Result<(), DurableDirError> {
        let parent = path.parent().ok_or(DurableDirError::NoParent)?;
        match OwnerOnly::create_dir(path) {
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
            DurableDir::create(dir)?;
        }
        Ok(())
    }

    /// Makes the directory `path` only when nothing holds its name: `Exists` otherwise, as a
    /// run's directory is its own.
    pub fn create_new(path: &Path) -> Result<(), PathError<DurableDirCreateError>> {
        let create = || {
            let parent = path.parent().ok_or(DurableDirError::NoParent)?;
            match OwnerOnly::create_dir(path) {
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

    /// Removes the directory `path` and all it holds.
    pub fn remove(path: &Path) -> Result<(), PathError<DurableDirError>> {
        let remove = || {
            let parent = path.parent().ok_or(DurableDirError::NoParent)?;
            fs::remove_dir_all(path).map_err(DurableDirError::Remove)?;
            DurableName::sync_dir(parent).map_err(DurableDirError::SyncParent)
        };
        remove().map_err(PathError::at(path))
    }
}

#[cfg(test)]
mod tests;
