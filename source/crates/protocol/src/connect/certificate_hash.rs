use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use campfire_math::{Bytes32, NotHex};

/// The SHA-256 hash of a server's TLS certificate: the client checks the certificate against it,
/// and signs it in its connect answer. It reads and writes as 64 lowercase hex digits, as a
/// listing gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CertificateHash(Bytes32);

impl CertificateHash {
    pub const fn new(bytes: [u8; 32]) -> CertificateHash {
        CertificateHash(Bytes32::new(bytes))
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

impl fmt::Display for CertificateHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for CertificateHash {
    type Err = NotHex;

    fn from_str(text: &str) -> Result<CertificateHash, NotHex> {
        text.parse().map(CertificateHash)
    }
}
