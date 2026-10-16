use ring::hkdf;
use zeroize::Zeroizing;

/// A secret a server's endpoint derives from the secret of the server's own key, by HKDF-SHA256
/// ([RFC 5869](https://www.rfc-editor.org/rfc/rfc5869)) with no salt and the secret's label: one
/// key, kept in one file, gives both, so neither can be lost alone (design 15, D6, D8).
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
            Self::ResetKey => b"campfire/quic-reset/v1",
        }
    }

    /// This secret of the server whose own key's secret is `secret`.
    pub(crate) fn of(self, secret: &Zeroizing<[u8; 32]>) -> Zeroizing<[u8; 32]> {
        let key = hkdf::Salt::new(hkdf::HKDF_SHA256, &[]).extract(secret.as_slice());
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

    #[test]
    fn each_secret_is_an_independent_hkdfs() {
        // Python's `hmac`: PRK = HMAC-SHA256(32 zero bytes, secret), then each secret is
        // HMAC-SHA256(PRK, label ‖ 0x01), the one block of 32 bytes; the secret is 0x00 to 0x1f.
        let secret = Zeroizing::new(array::from_fn(|index| u8::try_from(index).unwrap()));
        let tls_seed = [
            0xef, 0xe4, 0x99, 0xab, 0xef, 0x5f, 0x22, 0x1c, 0x77, 0xa3, 0x57, 0x06, 0x1d, 0x5a,
            0x76, 0x34, 0x94, 0x8a, 0x37, 0xb0, 0x35, 0x3a, 0x59, 0xce, 0x30, 0xfd, 0xf9, 0xc0,
            0x51, 0x63, 0xc5, 0x31,
        ];
        let reset_key = [
            0xc9, 0xca, 0xbb, 0xe5, 0x46, 0x2a, 0x9e, 0x35, 0xc6, 0x6e, 0xb1, 0x84, 0xb6, 0x7d,
            0xf4, 0x9b, 0x1a, 0xad, 0x85, 0xae, 0x89, 0x02, 0xf7, 0x8e, 0xb1, 0x0c, 0x2f, 0x5d,
            0xab, 0x7c, 0x5b, 0xab,
        ];
        assert_eq!(*DerivedSecret::TlsSeed.of(&secret), tls_seed);
        assert_eq!(*DerivedSecret::ResetKey.of(&secret), reset_key);
    }
}
