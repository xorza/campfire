use std::fs;
use std::io::{self, BufWriter, IntoInnerError, Write};
use std::path::{Path, PathBuf};

use crate::durable_file::error::{DurableCreateError, DurableError};
use crate::path_error::PathError;
use campfire_platform::DurableName;
use campfire_platform::UserOnly;

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

    /// Replaces the file at `path`, or makes it, as `write` does, with the bytes `fill` writes
    /// into it as it goes, through a buffer, so a file of any length takes no more memory than
    /// the buffer: a long log, as a session's published one. An error `fill` gives fails the write
    /// as `Write`.
    pub fn write_with(
        path: &Path,
        fill: &mut dyn FnMut(&mut dyn Write) -> io::Result<()>,
    ) -> Result<(), PathError<DurableError>> {
        let mut durable = || {
            let mut temporary = Self::fill_beside(path, ".part", fill)?;
            DurableName::rename(&temporary.path, path).map_err(DurableError::Rename)?;
            temporary.named = true;
            DurableName::sync_dir(temporary.directory).map_err(DurableError::SyncDir)
        };
        durable().map_err(PathError::at(path))
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
        Self::fill_beside(path, suffix, &mut |file| file.write_all(bytes))
    }

    /// Writes what `fill` writes, through a buffer, to a new temporary file beside `path`, as
    /// `write_beside` writes its bytes, and syncs it.
    fn fill_beside<'a>(
        path: &'a Path,
        suffix: &str,
        fill: &mut dyn FnMut(&mut dyn Write) -> io::Result<()>,
    ) -> Result<Temporary<'a>, DurableError> {
        let directory = path.parent().ok_or(DurableError::NoName)?;
        let temporary = Self::beside(path, suffix)?;
        match fs::remove_file(&temporary) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(DurableError::RemoveStale(error)),
        }
        let file = UserOnly::create_new(&temporary).map_err(DurableError::Create)?;
        let temporary = Temporary {
            directory,
            path: temporary,
            named: false,
        };
        let mut buffered = BufWriter::new(file);
        // The file closes as its sync ends, or with the buffer on an error, before the temporary
        // is named or removed, so no handle of it is open then.
        let written = fill(&mut buffered)
            .and_then(|()| buffered.into_inner().map_err(IntoInnerError::into_error))
            .map_err(DurableError::Write)
            .and_then(|file| file.sync_all().map_err(DurableError::Sync));
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
