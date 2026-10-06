use std::io::ErrorKind;
use std::path::Path;

use campfire_protocol::secp256k1::Keypair;
use campfire_protocol::{Nsec, RandomKey};
use campfire_store::{SecretFile, SecretReadError};

use crate::sim_server::key_file::error::KeyFileError;

pub(crate) mod error;

/// A secret key in a file: its `nsec` text in a secret file, only its owner may read.
#[derive(Debug)]
pub struct KeyFile;

impl KeyFile {
    /// The key in the file at `path`, whose mode is checked before its key is read.
    pub fn read(path: &Path) -> Result<Keypair, KeyFileError> {
        let bytes = SecretFile::read(path).map_err(KeyFileError::Read)?;
        Nsec::decode(&bytes).map_err(KeyFileError::NotNsec)
    }

    /// Writes `key` to the file at `path`, durably, its owner's only.
    pub fn write(path: &Path, key: &Keypair) -> Result<(), KeyFileError> {
        SecretFile::write(path, Nsec::encode(key).as_bytes()).map_err(KeyFileError::Write)
    }

    /// The key in the file at `path`, or, when there is no file, a new key from `fill`'s random
    /// bytes, written there first.
    pub fn read_or_create(path: &Path, fill: fn(&mut [u8; 32])) -> Result<Keypair, KeyFileError> {
        match KeyFile::read(path) {
            Err(KeyFileError::Read(SecretReadError::Read(error)))
                if error.kind() == ErrorKind::NotFound =>
            {
                let key = RandomKey::generate(fill);
                KeyFile::write(path, &key)?;
                Ok(key)
            }
            read => read,
        }
    }
}

#[cfg(test)]
mod tests;
