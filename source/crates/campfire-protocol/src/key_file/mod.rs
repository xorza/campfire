use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::Path;

use nostr::key::SecretKey as NostrKey;
use nostr::nips::nip19::{FromBech32, ToBech32};
use secp256k1::{Keypair, Secp256k1, SecretKey};

use crate::durable_file::DurableFile;
use crate::key_file::error::KeyFileError;

pub(crate) mod error;

/// A secret key in a file: a NIP-19 `nsec` and a newline, only its owner may read. On Unix a file
/// others may read is refused, as OpenSSH refuses a key with loose permissions; Windows has no
/// modes, and no check runs there.
#[derive(Debug)]
pub struct KeyFile;

impl KeyFile {
    /// The key in the file at `path`, whose mode is checked before its key is read.
    pub fn read(path: &Path) -> Result<Keypair, KeyFileError> {
        let mut file = File::open(path).map_err(KeyFileError::Read)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = file
                .metadata()
                .map_err(KeyFileError::Read)?
                .permissions()
                .mode();
            if mode & 0o077 != 0 {
                return Err(KeyFileError::Exposed { mode: mode & 0o777 });
            }
        }
        let mut text = String::new();
        file.read_to_string(&mut text).map_err(KeyFileError::Read)?;
        let key = NostrKey::from_bech32(text.trim()).map_err(KeyFileError::NotNsec)?;
        let secret = SecretKey::from_byte_array(&key.to_secret_bytes())
            .expect("an nsec holds a valid secret key");
        Ok(Keypair::from_secret_key(&Secp256k1::new(), &secret))
    }

    /// Writes `key` to the file at `path`, durably, its owner's only.
    pub fn write(path: &Path, key: &Keypair) -> Result<(), KeyFileError> {
        let secret =
            NostrKey::from_slice(&key.secret_bytes()).expect("a keypair's secret is a key");
        let Ok(nsec) = secret.to_bech32();
        DurableFile::write(path, format!("{nsec}\n").as_bytes()).map_err(KeyFileError::Write)
    }

    /// The key in the file at `path`, or, when there is no file, a new key from `fill`'s random
    /// bytes, written there first.
    pub fn read_or_create(path: &Path, fill: fn(&mut [u8; 32])) -> Result<Keypair, KeyFileError> {
        match KeyFile::read(path) {
            Err(KeyFileError::Read(error)) if error.kind() == ErrorKind::NotFound => {
                let key = loop {
                    let mut secret = [0; 32];
                    fill(&mut secret);
                    if let Ok(secret) = SecretKey::from_byte_array(&secret) {
                        break Keypair::from_secret_key(&Secp256k1::new(), &secret);
                    }
                };
                KeyFile::write(path, &key)?;
                Ok(key)
            }
            read => read,
        }
    }
}

#[cfg(test)]
mod tests;
