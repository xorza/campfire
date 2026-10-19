use zeroize::Zeroizing;

/// A server's secret: 32 bytes from which its raw key and its reset key both derive, so one file
/// holds its identity (13-net-stack/udp.md, One server secret derives the raw key and the reset
/// key).
pub struct ServerSecret(Zeroizing<[u8; 32]>);

impl ServerSecret {
    /// The secret of `bytes`, as an app reads them from its file.
    pub const fn new(bytes: Zeroizing<[u8; 32]>) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &Zeroizing<[u8; 32]> {
        &self.0
    }
}

/// The bytes take no part in it, so no log holds them.
impl core::fmt::Debug for ServerSecret {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ServerSecret(..)")
    }
}
