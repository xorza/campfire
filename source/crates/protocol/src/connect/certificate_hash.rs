use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::connect::error::NotHex;
use crate::hex;

/// The SHA-256 hash of a server's TLS certificate: the client checks the certificate against it,
/// and signs it in its connect answer. It reads and writes as 64 lowercase hex digits, as a
/// listing gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CertificateHash([u8; 32]);

impl CertificateHash {
    pub const fn new(bytes: [u8; 32]) -> CertificateHash {
        CertificateHash(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for CertificateHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&hex::encode(&self.0))
    }
}

impl FromStr for CertificateHash {
    type Err = NotHex;

    fn from_str(text: &str) -> Result<CertificateHash, NotHex> {
        hex::decode(text).map(CertificateHash).ok_or(NotHex)
    }
}
