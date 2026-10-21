use campfire_common::Secret;
use ring::hkdf;
use zeroize::Zeroizing;

use crate::server_secret::ServerSecret;

/// A secret a server's endpoint derives from its [`ServerSecret`], by HKDF-SHA256
/// ([RFC 5869](https://www.rfc-editor.org/rfc/rfc5869)) with no salt and the secret's label
/// (11-net-stack/udp.md, One server secret derives the raw key and the reset key). The TLS seed's
/// label is the one QUIC's endpoint gave it, so a server's key file keeps its published key
/// (11-net-stack/udp.md, One server secret derives the raw key and the reset key).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DerivedSecret {
    /// The seed of the Ed25519 key the server presents to TLS.
    TlsSeed,
    /// The key of the endpoint's stateless resets.
    ResetKey,
}

impl DerivedSecret {
    const fn label(self) -> &'static [u8] {
        match self {
            Self::TlsSeed => b"campfire/tls/v1",
            Self::ResetKey => b"campfire/udp-reset/v1",
        }
    }

    /// This secret of the server whose secret is `secret`.
    pub(crate) fn of(self, secret: &ServerSecret) -> Secret<32> {
        let key = hkdf::Salt::new(hkdf::HKDF_SHA256, &[]).extract(secret.as_bytes().as_slice());
        let mut derived = Zeroizing::new([0; 32]);
        key.expand(&[self.label()], hkdf::HKDF_SHA256)
            .expect("32 bytes are one block of HKDF-SHA256")
            .fill(derived.as_mut_slice())
            .expect("the output is the length expand was given");
        Secret::new(*derived)
    }
}

#[cfg(test)]
mod tests;
