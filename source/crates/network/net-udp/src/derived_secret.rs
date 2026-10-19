use ring::hkdf;
use zeroize::Zeroizing;

use crate::server_secret::ServerSecret;

/// A secret a server's endpoint derives from its [`ServerSecret`], by HKDF-SHA256
/// ([RFC 5869](https://www.rfc-editor.org/rfc/rfc5869)) with no salt and the secret's label
/// (13-net-stack/udp.md, One server secret derives the raw key and the reset key).
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
            Self::TlsSeed => b"campfire/udp/tls/v1",
            Self::ResetKey => b"campfire/udp/reset/v1",
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
    /// HMAC-SHA256(PRK, label ‖ 0x01), the one block of 32 bytes; the secret is 0x00 to 0x1f.
    #[test]
    fn each_secret_is_an_independent_hkdfs() {
        let secret = ServerSecret::new(Zeroizing::new(array::from_fn(|index| {
            u8::try_from(index).unwrap()
        })));
        let tls_seed = [
            0x75, 0xe1, 0x82, 0x4a, 0xbc, 0x91, 0x69, 0x83, 0x26, 0xe2, 0x8d, 0xf2, 0xad, 0x29,
            0xaf, 0xb9, 0xf2, 0xb4, 0xb8, 0xba, 0x08, 0x2b, 0xb1, 0xf2, 0x0d, 0xe1, 0x4b, 0x9f,
            0x08, 0x19, 0xfc, 0x01,
        ];
        let reset_key = [
            0x5f, 0xed, 0x55, 0x51, 0x33, 0x1d, 0xff, 0xc0, 0xe5, 0xb4, 0x40, 0x8c, 0xac, 0xbd,
            0xe5, 0xd8, 0x24, 0xe5, 0xf6, 0xc4, 0x16, 0xe1, 0x16, 0xb1, 0x81, 0x58, 0x71, 0x64,
            0xd8, 0x2e, 0x6b, 0x84,
        ];
        assert_eq!(*DerivedSecret::TlsSeed.of(&secret), tls_seed);
        assert_eq!(*DerivedSecret::ResetKey.of(&secret), reset_key);
    }
}
