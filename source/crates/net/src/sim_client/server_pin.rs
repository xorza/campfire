use campfire_protocol::CertificateHash;

/// The server a player means to reach, as its listing names it: its key, which the offered terms
/// must name, and the hash of its certificate, which the transport checks and the join answer
/// signs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerPin {
    /// The server's x-only public key.
    pub key: [u8; 32],
    pub certificate: CertificateHash,
}
