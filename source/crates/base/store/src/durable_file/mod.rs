use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::durable_file::error::{DurableCreateError, DurableError};
use crate::path_error::PathError;
use crate::platform::durable_name::DurableName;
use crate::platform::user_only::UserOnly;

pub(crate) mod error;

/// A file written whole or not at all, which a crash leaves either as it was or as written: the
/// bytes go to a temporary file beside it, which is synced and given its name durably, so the
/// new name survives a crash as well as the bytes. The temporary file is always made new, a stale
/// one a crash left removed first, so the file is its user's only, whatever access the stale one
/// gave; a write that fails removes its own.
#[derive(Debug)]
pub struct DurableFile;

/// A temporary file beside the file it is written for, removed as it drops unless it was given
/// that file's name.
#[derive(Debug)]
struct Temporary<'a> {
    directory: &'a Path,
    path: PathBuf,
    named: bool,
}

impl DurableFile {
    /// Replaces the file at `path`, or makes it, with `bytes`, through the one temporary
    /// `<name>.part`, so the writers of one name take turns by their holder's lock, as a data
    /// directory's.
    pub fn write(path: &Path, bytes: &[u8]) -> Result<(), PathError<DurableError>> {
        Self::write_case(path, bytes).map_err(PathError::at(path))
    }

    /// `write`, whose error its caller names the path of.
    pub(crate) fn write_case(path: &Path, bytes: &[u8]) -> Result<(), DurableError> {
        let mut temporary = Self::write_beside(path, ".part", bytes)?;
        DurableName::rename(&temporary.path, path).map_err(DurableError::Rename)?;
        temporary.named = true;
        DurableName::sync_dir(temporary.directory).map_err(DurableError::SyncDir)
    }

    /// Makes the file at `path` with `bytes` only when no file holds its name, and leaves one
    /// that does as it is: of two writers that make one file at once, one makes it, and the
    /// other gets `Exists`. The writers of one name take turns by the lock of `<name>.lock`
    /// beside it, an empty file that stays, so each writes the one temporary `<name>.new`, and
    /// the next create removes one a crash left.
    pub fn create_new(path: &Path, bytes: &[u8]) -> Result<(), PathError<DurableCreateError>> {
        Self::create_new_case(path, bytes).map_err(PathError::at(path))
    }

    fn create_new_case(path: &Path, bytes: &[u8]) -> Result<(), DurableCreateError> {
        let lock =
            UserOnly::open_or_create(&Self::beside(path, ".lock")?).map_err(DurableError::Lock)?;
        lock.lock().map_err(DurableError::Lock)?;
        let mut temporary = Self::write_beside(path, ".new", bytes)?;
        match DurableName::create(&temporary.path, path) {
            Ok(()) => temporary.named = true,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                return Err(DurableCreateError::Exists);
            }
            Err(error) => return Err(DurableError::Rename(error).into()),
        }
        DurableName::sync_dir(temporary.directory).map_err(DurableError::SyncDir)?;
        Ok(())
    }

    /// The path beside `path` whose name is `path`'s with `suffix`, in `path`'s directory.
    fn beside(path: &Path, suffix: &str) -> Result<PathBuf, DurableError> {
        let (Some(directory), Some(name)) = (path.parent(), path.file_name()) else {
            return Err(DurableError::NoName);
        };
        let mut beside = name.to_owned();
        beside.push(suffix);
        Ok(directory.join(beside))
    }

    /// Writes `bytes` to a new temporary file beside `path`, whose name is `path`'s with
    /// `suffix`, and syncs it; a stale one of that name, which a crash left, is removed first.
    fn write_beside<'a>(
        path: &'a Path,
        suffix: &str,
        bytes: &[u8],
    ) -> Result<Temporary<'a>, DurableError> {
        let directory = path.parent().ok_or(DurableError::NoName)?;
        let temporary = Self::beside(path, suffix)?;
        match fs::remove_file(&temporary) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(DurableError::RemoveStale(error)),
        }
        let mut file = UserOnly::create_new(&temporary).map_err(DurableError::Create)?;
        let temporary = Temporary {
            directory,
            path: temporary,
            named: false,
        };
        let written = file
            .write_all(bytes)
            .map_err(DurableError::Write)
            .and_then(|()| file.sync_all().map_err(DurableError::Sync));
        // Closed before the temporary is named or removed, so no handle of it is open then.
        drop(file);
        written?;
        Ok(temporary)
    }
}

/// Removes the temporary file a write left unnamed, as each of its errors does; a removal that
/// fails leaves it for the next write of its name, which removes a stale one first.
impl Drop for Temporary<'_> {
    fn drop(&mut self) {
        if !self.named {
            fs::remove_file(&self.path).ok();
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
