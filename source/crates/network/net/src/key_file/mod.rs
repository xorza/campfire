use campfire_protocol::{Entropy, MAX_NSEC_FILE_LEN, decode_nsec, encode_nsec};
use campfire_store::{DurableCreateError, PathError, ReadError, SecretFile};
use secp256k1::Keypair;

use crate::key_file::error::KeyFileError;

pub(crate) mod error;

/// A secret key in a file: its `nsec` text in a secret file, only its owner may read. A key is
/// only ever created, never replaced.
#[derive(Debug)]
pub struct KeyFile;

impl KeyFile {
    /// The key in `file`, whose access is checked before its key is read.
    pub fn read(file: &SecretFile) -> Result<Keypair, KeyFileError> {
        let bytes = file.read(MAX_NSEC_FILE_LEN).map_err(KeyFileError::Read)?;
        decode_nsec(&bytes).map_err(KeyFileError::NotNsec)
    }

    /// The key in `file`, or, when there is no file, a new key from `entropy`'s random bytes,
    /// created there first; when another process created it meanwhile, its key.
    pub fn read_or_create(file: &SecretFile, entropy: Entropy) -> Result<Keypair, KeyFileError> {
        match Self::read(file) {
            Err(KeyFileError::Read(PathError {
                error: ReadError::Missing,
                ..
            })) => {
                let key = entropy.keypair();
                match file.create_new(encode_nsec(&key).as_bytes()) {
                    Ok(()) => Ok(key),
                    Err(PathError {
                        error: DurableCreateError::Exists,
                        ..
                    }) => Self::read(file),
                    Err(PathError {
                        path,
                        error: DurableCreateError::Make(error),
                    }) => Err(KeyFileError::Write(PathError { path, error })),
                }
            }
            read => read,
        }
    }
}

#[cfg(test)]
mod tests;
