use std::str::FromStr;

use derive_more::Display;
use serde::{Deserialize, Serialize};

use crate::bytes32::Bytes32;
use crate::bytes32::error::NotHex;

/// The Ed25519 public key a server presents to TLS with no certificate (design 15, D6): the
/// client takes this key and no other, and signs it in its connect answer. It reads and writes as
/// 64 lowercase hex digits, as a listing gives it.
#[derive(
    Debug, Display, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct TlsKey(Bytes32);

impl TlsKey {
    pub const fn new(bytes: [u8; 32]) -> TlsKey {
        TlsKey(Bytes32::new(bytes))
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

impl FromStr for TlsKey {
    type Err = NotHex;

    fn from_str(text: &str) -> Result<TlsKey, NotHex> {
        text.parse().map(TlsKey)
    }
}
