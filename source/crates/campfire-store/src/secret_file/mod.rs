use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::durable_file::DurableFile;
use crate::durable_file::error::DurableError;
use crate::secret_file::error::SecretReadError;

pub(crate) mod error;

/// A file only its owner may read, as a secret key's. On Unix a file others may read is refused,
/// as OpenSSH refuses a key with loose permissions; Windows has no modes, and no check runs
/// there.
#[derive(Debug)]
pub struct SecretFile;

impl SecretFile {
    /// The bytes of the file at `path`, whose mode is checked before they are read.
    pub fn read(path: &Path) -> Result<Vec<u8>, SecretReadError> {
        let mut file = File::open(path).map_err(SecretReadError::Read)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = file
                .metadata()
                .map_err(SecretReadError::Read)?
                .permissions()
                .mode();
            if mode & 0o077 != 0 {
                return Err(SecretReadError::Exposed { mode: mode & 0o777 });
            }
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

#[cfg(test)]
mod tests;
