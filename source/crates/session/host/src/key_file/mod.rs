use campfire_protocol::{Entropy, MAX_NSEC_FILE_LEN, Signer, decode_nsec, encode_nsec};
use campfire_store::{DurableCreateError, PathError, ReadError, SecretFile};

use crate::key_file::error::KeyFileError;

pub(crate) mod error;

/// A secret key in a file: its `nsec` text in a secret file, only its owner may read. A key is
/// only ever created, never replaced.
#[derive(Debug)]
pub struct KeyFile;

impl KeyFile {
    /// The signer of the key in `file`, whose access is checked before its key is read, and
    /// whose signatures draw on `entropy`. The key goes straight into its signer, its one owner.
    pub fn read(file: &SecretFile, entropy: Entropy) -> Result<Signer, KeyFileError> {
        let bytes = file.read(MAX_NSEC_FILE_LEN).map_err(KeyFileError::Read)?;
        let key = decode_nsec(&bytes).map_err(KeyFileError::NotNsec)?;
        Ok(Signer::new(key, entropy))
    }

    /// The signer of the key in `file`, as `read` gives it, or, when there is no file, of a new
    /// key from `entropy`'s random bytes, created there first; when another process created it
    /// meanwhile, of its key.
    pub fn read_or_create(file: &SecretFile, entropy: Entropy) -> Result<Signer, KeyFileError> {
        match Self::read(file, entropy) {
            Err(KeyFileError::Read(PathError {
                error: ReadError::Missing,
                ..
            })) => {
                let signer = Signer::new(entropy.keypair(), entropy);
                match file.create_new(encode_nsec(&signer).as_bytes()) {
                    Ok(()) => Ok(signer),
                    Err(PathError {
                        error: DurableCreateError::Exists,
                        ..
                    }) => Self::read(file, entropy),
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
