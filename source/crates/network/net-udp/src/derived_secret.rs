use ring::hkdf;
use zeroize::Zeroizing;

use crate::server_secret::ServerSecret;

/// A secret a server's endpoint derives from its [`ServerSecret`], by HKDF-SHA256
/// ([RFC 5869](https://www.rfc-editor.org/rfc/rfc5869)) with no salt and the secret's label
/// (13-net-stack/udp.md, One server secret derives the raw key and the reset key). The TLS seed's
/// label is the one QUIC's endpoint gave it, so a server's key file keeps its published key
/// (14-workspace-alignment.md, The UDP labels keep the published key).
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
    pub(crate) fn of(self, secret: &ServerSecret) -> Zeroizing<[u8; 32]> {
        let key = hkdf::Salt::new(hkdf::HKDF_SHA256, &[]).extract(secret.as_bytes().as_slice());
        let mut derived = Zeroizing::new([0; 32]);
        key.expand(&[self.label()], hkdf::HKDF_SHA256)
            .expect("32 bytes are one block of HKDF-SHA256")
            .fill(derived.as_mut_slice())
            .expect("the output is the length expand was given");
        derived
    }
}

#[cfg(test)]
mod tests {
    use std::array;

    use super::*;

    /// Python's `hmac`: PRK = HMAC-SHA256(32 zero bytes, secret), then each secret is
    /// HMAC-SHA256(PRK, label ‖ 0x01), the one block of 32 bytes; the secret is 0x00 to 0x1f. The
    /// TLS seed is the one QUIC's endpoint derived from the same secret, so the key it gives does
    /// not change.
    #[test]
    fn each_secret_is_an_independent_hkdfs() {
        let secret = ServerSecret::new(Zeroizing::new(array::from_fn(|index| {
            u8::try_from(index).unwrap()
        })));
        let tls_seed = [
            0xef, 0xe4, 0x99, 0xab, 0xef, 0x5f, 0x22, 0x1c, 0x77, 0xa3, 0x57, 0x06, 0x1d, 0x5a,
            0x76, 0x34, 0x94, 0x8a, 0x37, 0xb0, 0x35, 0x3a, 0x59, 0xce, 0x30, 0xfd, 0xf9, 0xc0,
            0x51, 0x63, 0xc5, 0x31,
        ];
        let reset_key = [
            0x97, 0x7e, 0x2b, 0x60, 0xb4, 0xc2, 0xb7, 0x53, 0x36, 0x0e, 0x38, 0x87, 0xd8, 0xf4,
            0x99, 0x02, 0x94, 0xc7, 0x7f, 0x79, 0x64, 0x3a, 0xcd, 0xdc, 0x29, 0xb7, 0x95, 0xfb,
            0x5d, 0x31, 0xd1, 0xab,
        ];
        assert_eq!(*DerivedSecret::TlsSeed.of(&secret), tls_seed);
        assert_eq!(*DerivedSecret::ResetKey.of(&secret), reset_key);
    }
}
