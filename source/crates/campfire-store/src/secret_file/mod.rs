use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::durable_file::DurableFile;
use crate::durable_file::error::DurableError;
use crate::platform::owner_only::OwnerOnly;
use crate::secret_file::error::SecretReadError;

pub(crate) mod error;

/// A file only its owner may open, as a secret key's: a file others may open is refused, as
/// OpenSSH refuses a key with loose permissions.
#[derive(Debug)]
pub struct SecretFile;

impl SecretFile {
    /// The bytes of the file at `path`, whose access is checked before they are read.
    pub fn read(path: &Path) -> Result<Vec<u8>, SecretReadError> {
        let mut file = File::open(path).map_err(SecretReadError::Read)?;
        if let Some(exposure) = OwnerOnly::exposure(&file).map_err(SecretReadError::Read)? {
            return Err(SecretReadError::Exposed(exposure));
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(SecretReadError::Read)?;
        Ok(bytes)
    }

    /// Writes `bytes` to the file at `path`, durably, its owner's only.
    pub fn write(path: &Path, bytes: &[u8]) -> Result<(), DurableError> {
        DurableFile::write(path, bytes)
    }
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use std::path::Path;

    use crate::platform::owner_only::OwnerOnly;
    use crate::secret_file::SecretFile;

    impl SecretFile {
        /// Lets other users read the file at `path`: a test's secret file that is not its
        /// owner's only.
        pub fn expose(path: &Path) {
            OwnerOnly::expose(path);
        }
    }
}

#[cfg(test)]
mod tests;
