use std::fs::{File, TryLockError};
use std::path::{Path, PathBuf};

use crate::data_dir::error::DataDirError;
use crate::durable_dir::DurableDir;
use crate::path_error::PathError;
use crate::platform::user_only::UserOnly;

pub(crate) mod error;

/// A data directory, held locked: made durably when missing, its user's only, and locked by the
/// exclusive lock on its `lock` file until dropped, so a second holder is refused. One others may
/// open is refused, as Tor refuses its `DataDirectory`: who may write it may remove a secret in
/// it, which its holder then makes new, or replace its other files. A data
/// directory is made only by taking its lock, so holding one proves the lock is held, and no
/// code writes into a data directory it has not locked. Its holder's layout names the paths in it.
#[derive(Debug)]
pub struct DataDir {
    path: PathBuf,
    /// Holds the lock on `lock` until dropped.
    _lock: File,
}

impl DataDir {
    /// The data directory at `path`, made when missing, and locked.
    pub fn open(path: &Path) -> Result<Self, DataDirError> {
        DurableDir::create_all(path).map_err(DataDirError::Create)?;
        let exposure = UserOnly::dir_exposure(path)
            .map_err(|error| DataDirError::Inspect(PathError::at(path)(error)))?;
        if let Some(exposure) = exposure {
            return Err(DataDirError::Exposed {
                path: path.to_owned(),
                exposure,
            });
        }
        let lock_path = path.join("lock");
        let lock_error = |error| DataDirError::Lock(PathError::at(&lock_path)(error));
        let lock = UserOnly::open_or_create(&lock_path).map_err(lock_error)?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(DataDirError::Locked {
                    path: path.to_owned(),
                });
            }
            Err(TryLockError::Error(error)) => return Err(lock_error(error)),
        }
        Ok(Self {
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
