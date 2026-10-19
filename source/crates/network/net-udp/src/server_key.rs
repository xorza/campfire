/// The Ed25519 public key a server presents with no certificate (RFC 7250), which a client of
/// `ServerTrust::pinned` takes and no other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerKey([u8; 32]);

impl ServerKey {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}
