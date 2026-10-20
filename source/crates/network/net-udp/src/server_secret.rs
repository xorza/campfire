use campfire_common::Secret;

/// A server's secret: 32 bytes from which its raw key and its reset key both derive, so one file
/// holds its identity (11-net-stack/udp.md, One server secret derives the raw key and the reset
/// key).
#[derive(Debug)]
pub struct ServerSecret(Secret<32>);

impl ServerSecret {
    /// The secret of `bytes`, as an app reads them from its file.
    pub const fn new(bytes: Secret<32>) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}
