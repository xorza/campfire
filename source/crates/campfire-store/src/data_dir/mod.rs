use std::fs::{DirBuilder, File, TryLockError};
use std::path::{Path, PathBuf};

use crate::data_dir::error::DataDirError;

pub(crate) mod error;

/// A data directory, held locked: made when missing, its owner's only on Unix, and locked by the
/// exclusive lock on its `lock` file until dropped, so a second holder is refused. A data
/// directory is made only by taking its lock, so holding one proves the lock is held, and no
/// code writes into a data directory it has not locked. Its owner's layout names the paths in it.
#[derive(Debug)]
pub struct DataDir {
    path: PathBuf,
    /// Holds the lock on `lock` until dropped.
    _lock: File,
}

impl DataDir {
    /// The data directory at `path`, made when missing, and locked.
    pub fn open(path: &Path) -> Result<DataDir, DataDirError> {
        let mut builder = DirBuilder::new();
        builder.recursive(true);
        let mut options = File::options();
        options.write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
            builder.mode(0o700);
            options.mode(0o600);
        }
        builder.create(path).map_err(DataDirError::Create)?;
        let lock = options
            .open(path.join("lock"))
            .map_err(DataDirError::Lock)?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(DataDirError::Locked),
            Err(TryLockError::Error(error)) => return Err(DataDirError::Lock(error)),
        }
        Ok(DataDir {
            path: path.to_owned(),
            _lock: lock,
        })
    }

    /// The directory, which the paths in it start with.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests;
