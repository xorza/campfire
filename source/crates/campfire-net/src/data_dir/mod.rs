use std::fs::{DirBuilder, File, TryLockError};
use std::path::Path;

use campfire_protocol::secp256k1::Keypair;

use crate::data_dir::error::DataDirError;
use crate::key_file::KeyFile;

pub(crate) mod error;

/// A server's data directory, which it holds locked while it runs, so no second server writes
/// what it writes: its key, `server.nsec`, made when missing, and its sessions. On Unix the
/// directory it makes and its `lock` are its owner's only, as every file in it is.
#[derive(Debug)]
pub struct DataDir {
    pub key: Keypair,
    /// Holds the lock on `lock` until the server ends.
    _lock: File,
}

impl DataDir {
    /// The data directory at `path`, made when missing, its new key's bytes from `fill`.
    pub fn open(path: &Path, fill: fn(&mut [u8; 32])) -> Result<DataDir, DataDirError> {
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
        let key =
            KeyFile::read_or_create(&path.join("server.nsec"), fill).map_err(DataDirError::Key)?;
        Ok(DataDir { key, _lock: lock })
    }
}

#[cfg(test)]
mod tests {
    use std::{env, fs, process};

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
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
            assert_eq!((mode(&path), mode(&path.join("lock"))), (0o700, 0o600));
        }
        let key = first.key;
        drop(first);
        let again = DataDir::open(&path, |bytes| bytes.fill(4)).unwrap();
        assert_eq!(again.key, key);
        assert_eq!(key.secret_bytes(), [3; 32]);
        drop(again);
        fs::remove_dir_all(&path).unwrap();
    }
}
