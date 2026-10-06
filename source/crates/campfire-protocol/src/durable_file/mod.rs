use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

use crate::durable_file::error::DurableError;

pub(crate) mod error;

/// A file written whole or not at all, which a crash leaves either as it was or as written: the
/// bytes go to a temporary file beside it, which is synced and renamed over its name, and on Unix
/// the directory is synced too, so the new name survives a crash as well as the bytes. On Unix
/// the file is its owner's only, mode 0600. Windows syncs no directory, and its rename is the
/// last step.
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
        let mut file = DurableFile::create(&temporary).map_err(DurableError::Create)?;
        file.write_all(bytes).map_err(DurableError::Write)?;
        file.sync_all().map_err(DurableError::Sync)?;
        drop(file);
        fs::rename(&temporary, path).map_err(DurableError::Rename)?;
        DurableFile::sync_directory(directory).map_err(DurableError::SyncDirectory)
    }

    /// Makes the directory `path` when it is missing, its owner's only on Unix, and syncs its
    /// parent, so its name survives a crash.
    pub fn create_dir(path: &Path) -> Result<(), DurableError> {
        let parent = path.parent().ok_or(DurableError::NoName)?;
        let mut builder = fs::DirBuilder::new();
        builder.recursive(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists && path.is_dir() => {}
            Err(error) => return Err(DurableError::Create(error)),
        }
        DurableFile::sync_directory(parent).map_err(DurableError::SyncDirectory)
    }

    /// A new file at `path`, emptied if it is there, its owner's only on Unix.
    fn create(path: &Path) -> io::Result<File> {
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(path)
    }

    /// Syncs `directory`, so the names in it survive a crash; nothing on Windows, which opens no
    /// directory as a file.
    fn sync_directory(directory: &Path) -> io::Result<()> {
        if cfg!(unix) {
            let directory = if directory.as_os_str().is_empty() {
                Path::new(".")
            } else {
                directory
            };
            File::open(directory)?.sync_all()?;
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests;
