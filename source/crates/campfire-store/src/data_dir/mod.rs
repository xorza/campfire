use std::fs::{File, TryLockError};
use std::path::{Path, PathBuf};

use crate::data_dir::error::DataDirError;
use crate::platform::owner_only::OwnerOnly;

pub(crate) mod error;

/// A data directory, held locked: made when missing, its owner's only, and locked by the
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
        OwnerOnly::create_dir_all(path).map_err(DataDirError::Create)?;
        let lock = OwnerOnly::open_or_create(&path.join("lock")).map_err(DataDirError::Lock)?;
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
