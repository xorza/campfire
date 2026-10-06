use std::error::Error;
use std::fmt;
use std::fs::{self, File, TryLockError};
use std::io;
use std::path::Path;

use campfire_protocol::secp256k1::Keypair;
use campfire_protocol::{KeyFile, KeyFileError};

/// A server's data directory, which it holds locked while it runs, so no second server writes
/// what it writes: its key, `server.nsec`, made when missing, and its sessions.
#[derive(Debug)]
pub(crate) struct DataDir {
    pub(crate) key: Keypair,
    /// Holds the lock on `lock` until the server ends.
    _lock: File,
}

/// Why a data directory did not open.
#[derive(Debug)]
pub(crate) enum DataDirError {
    Create(io::Error),
    Lock(io::Error),
    /// Another server holds it.
    Locked,
    Key(KeyFileError),
}

impl DataDir {
    /// The data directory at `path`, made when missing, its new key's bytes from `fill`.
    pub(crate) fn open(path: &Path, fill: fn(&mut [u8; 32])) -> Result<DataDir, DataDirError> {
        fs::create_dir_all(path).map_err(DataDirError::Create)?;
        let lock = File::options()
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.join("lock"))
            .map_err(DataDirError::Lock)?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(DataDirError::Locked),
            Err(TryLockError::Error(error)) => return Err(DataDirError::Lock(error)),
        }
        let key =
            KeyFile::read_or_create(&path.join("server.nsec"), fill).map_err(DataDirError::Key)?;
        Ok(DataDir { key, _lock: lock })
    }
}

impl fmt::Display for DataDirError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataDirError::Create(error) => write!(f, "could not make the data directory: {error}"),
            DataDirError::Lock(error) => write!(f, "could not lock the data directory: {error}"),
            DataDirError::Locked => f.write_str("another server holds the data directory"),
            DataDirError::Key(error) => write!(f, "server.nsec: {error}"),
        }
    }
}

impl Error for DataDirError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            DataDirError::Create(error) | DataDirError::Lock(error) => Some(error),
            DataDirError::Locked => None,
            DataDirError::Key(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{env, process};

    use super::*;

    #[test]
    fn a_data_directory_keeps_its_key_and_holds_one_server() {
        let path = env::temp_dir().join(format!("campfire-data-dir-{}", process::id()));
        drop(fs::remove_dir_all(&path));
        let first = DataDir::open(&path, |bytes| bytes.fill(3)).unwrap();
        // A second server while the first runs is refused; after it, it takes the same key.
        assert!(matches!(
            DataDir::open(&path, |bytes| bytes.fill(4)),
            Err(DataDirError::Locked)
        ));
        let key = first.key;
        drop(first);
        let again = DataDir::open(&path, |bytes| bytes.fill(4)).unwrap();
        assert_eq!(again.key, key);
        assert_eq!(key.secret_bytes(), [3; 32]);
        drop(again);
        fs::remove_dir_all(&path).unwrap();
    }
}
