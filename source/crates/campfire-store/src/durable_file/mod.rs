use std::fs;
use std::io::{self, Write};
use std::path::Path;

use crate::durable_file::error::DurableError;
use crate::platform::durable_name::DurableName;
use crate::platform::owner_only::OwnerOnly;

pub(crate) mod error;

/// A file written whole or not at all, which a crash leaves either as it was or as written: the
/// bytes go to a temporary file beside it, which is synced and given its name durably, so the
/// new name survives a crash as well as the bytes. The temporary file is always made new, a stale
/// one a crash left removed first, so the file is its owner's only, whatever access the stale one
/// gave.
#[derive(Debug)]
pub struct DurableFile;

impl DurableFile {
    /// Replaces the file at `path`, or makes it, with `bytes`.
    pub fn write(path: &Path, bytes: &[u8]) -> Result<(), DurableError> {
        let (Some(directory), Some(name)) = (path.parent(), path.file_name()) else {
            return Err(DurableError::NoName);
        };
        let mut temporary = name.to_owned();
        temporary.push(".part");
        let temporary = directory.join(temporary);
        match fs::remove_file(&temporary) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(DurableError::RemoveStale(error)),
        }
        let mut file = OwnerOnly::create_new(&temporary).map_err(DurableError::Create)?;
        file.write_all(bytes).map_err(DurableError::Write)?;
        file.sync_all().map_err(DurableError::Sync)?;
        drop(file);
        DurableName::rename(&temporary, path).map_err(DurableError::Rename)?;
        DurableName::sync_dir(directory).map_err(DurableError::SyncDirectory)
    }

    /// Makes the directory `path` when it is missing, its owner's only, and syncs its parent, so
    /// its name survives a crash.
    pub fn create_dir(path: &Path) -> Result<(), DurableError> {
        let parent = path.parent().ok_or(DurableError::NoName)?;
        match OwnerOnly::create_dir(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists && path.is_dir() => {}
            Err(error) => return Err(DurableError::Create(error)),
        }
        DurableName::sync_dir(parent).map_err(DurableError::SyncDirectory)
    }

    /// Removes the directory `path` and all it holds, and syncs its parent, so it does not come
    /// back after a crash.
    pub fn remove_dir(path: &Path) -> Result<(), DurableError> {
        let parent = path.parent().ok_or(DurableError::NoName)?;
        fs::remove_dir_all(path).map_err(DurableError::Remove)?;
        DurableName::sync_dir(parent).map_err(DurableError::SyncDirectory)
    }
}

#[cfg(test)]
pub(crate) mod tests;
