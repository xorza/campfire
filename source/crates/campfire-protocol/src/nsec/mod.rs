use std::str;

use nostr::key::SecretKey as NostrKey;
use nostr::nips::nip19::{FromBech32, ToBech32};
use secp256k1::{Keypair, Secp256k1, SecretKey};
use zeroize::Zeroizing;

use crate::nsec::error::NsecError;

pub(crate) mod error;

/// A secret key's text in a key file: a NIP-19 `nsec` and a newline.
#[derive(Debug)]
pub struct Nsec;

/// The characters of an `nsec`: its prefix `nsec`, the separator `1`, the 32-byte key in 52
/// characters of 5 bits each, and a checksum of 6.
const NSEC_LEN: usize = 4 + 1 + (32_usize * 8).div_ceil(5) + 6;

impl Nsec {
    /// The most bytes a key file may hold: an `nsec`, and as many again of the white space around
    /// it, which `decode` trims and an editor may add.
    pub const MAX_FILE_LEN: usize = 2 * NSEC_LEN;

    /// The text of `key`, in a buffer zeroed when dropped, as is each copy of the key made on
    /// the way.
    pub fn encode(key: &Keypair) -> Zeroizing<String> {
        let bytes = Zeroizing::new(key.secret_bytes());
        let secret = NostrKey::from_slice(&bytes[..]).expect("a keypair's secret is a key");
        let Ok(nsec) = secret.to_bech32();
        let nsec = Zeroizing::new(nsec);
        // Sized whole, so the newline grows no buffer and leaves no copy behind.
        let mut text = Zeroizing::new(String::with_capacity(NSEC_LEN + 1));
        text.push_str(&nsec);
        text.push('\n');
        text
    }

    /// The key `bytes` hold: an `nsec`, with any white space around it.
    pub fn decode(bytes: &[u8]) -> Result<Keypair, NsecError> {
        let text = str::from_utf8(bytes).map_err(NsecError::NotText)?;
        let key = NostrKey::from_bech32(text.trim()).map_err(NsecError::NotNsec)?;
        let secret = SecretKey::from_byte_array(&key.to_secret_bytes())
            .expect("an nsec holds a valid secret key");
        Ok(Keypair::from_secret_key(&Secp256k1::new(), &secret))
    }
}

#[cfg(test)]
mod tests;
