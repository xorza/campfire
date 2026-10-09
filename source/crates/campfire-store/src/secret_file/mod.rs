use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::PathBuf;

use zeroize::Zeroizing;

use crate::durable_file::DurableFile;
use crate::durable_file::error::{DurableCreateError, DurableError};
use crate::platform::owner_only::OwnerOnly;
use crate::secret_file::error::SecretReadError;

pub(crate) mod error;

/// A secret's file, which only its owner may open, as a key's: made from its path where the path
/// enters, it reads and writes the file and gives no path back, so no other read of a secret
/// compiles. A read refuses a file others may open, as OpenSSH refuses a key with loose
/// permissions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretFile {
    path: PathBuf,
}

impl SecretFile {
    pub const fn at(path: PathBuf) -> SecretFile {
        SecretFile { path }
    }

    /// The bytes of the file, at most `max_len`, checked from its open handle before any is
    /// read, so the file checked is the file read; a link is followed. The buffer is allocated
    /// once, at `max_len + 1`, and never grows, so no copy of the secret stays in freed memory,
    /// and is zeroed when dropped.
    pub fn read(&self, max_len: usize) -> Result<Zeroizing<Vec<u8>>, SecretReadError> {
        let mut file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(SecretReadError::Missing);
            }
            // Windows opens no directory as a file, so the path's own kind says why it failed.
            Err(_) if fs::metadata(&self.path).is_ok_and(|metadata| !metadata.is_file()) => {
                return Err(SecretReadError::NotFile);
            }
            Err(error) => return Err(SecretReadError::Read(error)),
        };
        let metadata = file.metadata().map_err(SecretReadError::Read)?;
        if !metadata.is_file() {
            return Err(SecretReadError::NotFile);
        }
        if let Some(exposure) = OwnerOnly::exposure(&file).map_err(SecretReadError::Read)? {
            return Err(SecretReadError::Exposed(exposure));
        }
        let too_large = SecretReadError::TooLarge { max: max_len };
        if metadata.len() > u64::try_from(max_len).expect("a length fits u64") {
            return Err(too_large);
        }
        // Read into the buffer itself: `read_to_end` probes through a buffer on the stack, which
        // nothing zeroes.
        let mut bytes = Zeroizing::new(vec![0; max_len + 1]);
        let mut filled = 0;
        while filled < bytes.len() {
            match file.read(&mut bytes[filled..]) {
                Ok(0) => break,
                Ok(read) => filled += read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(SecretReadError::Read(error)),
            }
        }
        // The file grew since its metadata was read.
        if filled > max_len {
            return Err(too_large);
        }
        bytes.truncate(filled);
        Ok(bytes)
    }

    /// Makes the file with `bytes`, durably and its owner's only, and never replaces one: a file
    /// that holds its name is left as it is, `Exists`, so of two processes that make one secret
    /// at once, one makes it and the other reads it.
    pub fn create(&self, bytes: &[u8]) -> Result<(), DurableCreateError> {
        DurableFile::create(&self.path, bytes)
    }

    /// Replaces the file, or makes it, with `bytes`, durably and its owner's only: a secret its
    /// owner renews, under its data directory's lock.
    pub fn replace(&self, bytes: &[u8]) -> Result<(), DurableError> {
        DurableFile::write(&self.path, bytes)
    }
}

impl fmt::Display for SecretFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.path.display().fmt(f)
    }
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use crate::platform::owner_only::OwnerOnly;
    use crate::secret_file::SecretFile;

    impl SecretFile {
        /// Lets other users read the file: a test's secret file that is not its owner's only.
        pub fn expose(&self) {
            OwnerOnly::expose(&self.path);
        }
    }
}

#[cfg(test)]
mod tests;
