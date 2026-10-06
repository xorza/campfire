use std::str;

use nostr::key::SecretKey as NostrKey;
use nostr::nips::nip19::{FromBech32, ToBech32};
use secp256k1::{Keypair, Secp256k1, SecretKey};

use crate::nsec::error::NsecError;

pub(crate) mod error;

/// A secret key's text in a key file: a NIP-19 `nsec` and a newline.
#[derive(Debug)]
pub struct Nsec;

impl Nsec {
    /// The text of `key`.
    pub fn encode(key: &Keypair) -> String {
        let secret =
            NostrKey::from_slice(&key.secret_bytes()).expect("a keypair's secret is a key");
        let Ok(nsec) = secret.to_bech32();
        format!("{nsec}\n")
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
mod tests {
    use super::*;

    #[test]
    fn a_key_round_trips_and_what_is_no_nsec_is_refused() {
        let secret = SecretKey::from_byte_array(&[7; 32]).unwrap();
        let key = Keypair::from_secret_key(&Secp256k1::new(), &secret);
        let text = Nsec::encode(&key);
        assert!(text.starts_with("nsec1") && text.ends_with('\n'), "{text}");
        assert_eq!(Nsec::decode(text.as_bytes()).unwrap(), key);
        assert_eq!(
            Nsec::decode(format!("  {}  ", text.trim()).as_bytes()).unwrap(),
            key
        );
        // An npub, text that is no key, and bytes that are no text.
        for wrong in [
            &b"npub1sg6plzptd64u62a878hep2kev88swjh3tw00gjsfl8f237lmu63q0uf63m"[..],
            b"secret",
        ] {
            assert!(matches!(Nsec::decode(wrong), Err(NsecError::NotNsec(_))));
        }
        assert!(matches!(
            Nsec::decode(&[0xff, 0xfe]),
            Err(NsecError::NotText(_))
        ));
    }
}
