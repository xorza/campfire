use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::{process, thread};

use crate::durable_file::error::{DurableCreateError, DurableError};
use crate::path_error::PathError;
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

/// A temporary file written and synced in `directory`, which waits for its name.
#[derive(Debug)]
struct Written<'a> {
    directory: &'a Path,
    temporary: PathBuf,
}

impl DurableFile {
    /// Replaces the file at `path`, or makes it, with `bytes`.
    pub fn write(path: &Path, bytes: &[u8]) -> Result<(), PathError<DurableError>> {
        DurableFile::write_case(path, bytes).map_err(PathError::at(path))
    }

    /// `write`, whose error its caller names the path of.
    pub(crate) fn write_case(path: &Path, bytes: &[u8]) -> Result<(), DurableError> {
        let Written {
            directory,
            temporary,
        } = DurableFile::write_beside(path, ".part", bytes)?;
        DurableName::rename(&temporary, path).map_err(DurableError::Rename)?;
        DurableName::sync_dir(directory).map_err(DurableError::SyncDirectory)
    }

    /// Makes the file at `path` with `bytes` only when no file holds its name, and leaves one
    /// that does as it is: of two writers that make one file at once, one makes it, and the
    /// other gets `Exists`. Each writer's temporary file holds its process's and its thread's
    /// numbers in its name, so no writer removes or names another's.
    pub fn create(path: &Path, bytes: &[u8]) -> Result<(), PathError<DurableCreateError>> {
        DurableFile::create_case(path, bytes).map_err(PathError::at(path))
    }

    fn create_case(path: &Path, bytes: &[u8]) -> Result<(), DurableCreateError> {
        // `ThreadId` gives its number, unique while the process runs, only through `Debug`.
        let thread = format!("{:?}", thread::current().id());
        let thread: String = thread.chars().filter(char::is_ascii_digit).collect();
        let suffix = format!(".{}.{thread}.part", process::id());
        let Written {
            directory,
            temporary,
        } = DurableFile::write_beside(path, &suffix, bytes)?;
        match DurableName::create(&temporary, path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                fs::remove_file(&temporary).map_err(DurableError::RemoveTemporary)?;
                return Err(DurableCreateError::Exists);
            }
            Err(error) => return Err(DurableError::Rename(error).into()),
        }
        DurableName::sync_dir(directory).map_err(DurableError::SyncDirectory)?;
        Ok(())
    }

    /// Writes `bytes` to a new temporary file beside `path`, whose name is `path`'s with
    /// `suffix`, and syncs it; a stale one of that name, which a crash left, is removed first.
    fn write_beside<'a>(
        path: &'a Path,
        suffix: &str,
        bytes: &[u8],
    ) -> Result<Written<'a>, DurableError> {
        let (Some(directory), Some(name)) = (path.parent(), path.file_name()) else {
            return Err(DurableError::NoName);
        };
        let mut temporary = name.to_owned();
        temporary.push(suffix);
        let temporary = directory.join(temporary);
        match fs::remove_file(&temporary) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(DurableError::RemoveStale(error)),
        }
        let mut file = OwnerOnly::create_new(&temporary).map_err(DurableError::Create)?;
        file.write_all(bytes).map_err(DurableError::Write)?;
        file.sync_all().map_err(DurableError::Sync)?;
        Ok(Written {
            directory,
            temporary,
        })
    }

    /// Makes the directory `path` when it is missing, its owner's only, and syncs its parent, so
    /// its name survives a crash.
    pub fn create_dir(path: &Path) -> Result<(), PathError<DurableError>> {
        DurableFile::create_dir_case(path).map_err(PathError::at(path))
    }

    fn create_dir_case(path: &Path) -> Result<(), DurableError> {
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
    pub fn remove_dir(path: &Path) -> Result<(), PathError<DurableError>> {
        DurableFile::remove_dir_case(path).map_err(PathError::at(path))
    }

    fn remove_dir_case(path: &Path) -> Result<(), DurableError> {
        let parent = path.parent().ok_or(DurableError::NoName)?;
        fs::remove_dir_all(path).map_err(DurableError::Remove)?;
        DurableName::sync_dir(parent).map_err(DurableError::SyncDirectory)
    }
}

#[cfg(test)]
pub(crate) mod tests;
