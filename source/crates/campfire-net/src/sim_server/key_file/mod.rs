use campfire_protocol::secp256k1::Keypair;
use campfire_protocol::{Nsec, RandomKey};
use campfire_store::{DurableCreateError, PathError, ReadError, SecretFile};

use crate::sim_server::key_file::error::KeyFileError;

pub(crate) mod error;

/// A secret key in a file: its `nsec` text in a secret file, only its owner may read. A key is
/// only ever created, never replaced.
#[derive(Debug)]
pub struct KeyFile;

impl KeyFile {
    /// The key in `file`, whose access is checked before its key is read.
    pub fn read(file: &SecretFile) -> Result<Keypair, KeyFileError> {
        let bytes = file.read(Nsec::MAX_FILE_LEN).map_err(KeyFileError::Read)?;
        Nsec::decode(&bytes).map_err(KeyFileError::NotNsec)
    }

    /// The key in `file`, or, when there is no file, a new key from `entropy`'s random bytes,
    /// created there first; when another process created it meanwhile, its key.
    pub fn read_or_create(
        file: &SecretFile,
        entropy: fn(&mut [u8; 32]),
    ) -> Result<Keypair, KeyFileError> {
        match KeyFile::read(file) {
            Err(KeyFileError::Read(PathError {
                error: ReadError::Missing,
                ..
            })) => {
                let key = RandomKey::generate(entropy);
                match file.create(Nsec::encode(&key).as_bytes()) {
                    Ok(()) => Ok(key),
                    Err(PathError {
                        error: DurableCreateError::Exists,
                        ..
                    }) => KeyFile::read(file),
                    Err(PathError {
                        path,
                        error: DurableCreateError::Write(error),
                    }) => Err(KeyFileError::Write(PathError { path, error })),
                }
            }
            read => read,
        }
    }
}

#[cfg(test)]
mod tests;
