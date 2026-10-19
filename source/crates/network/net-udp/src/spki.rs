use crate::server_key::ServerKey;

/// The `SubjectPublicKeyInfo` of an Ed25519 key before its 32 bytes ([RFC 8410] §4): the
/// algorithm `id-Ed25519` and the bit string's header.
///
/// [RFC 8410]: https://www.rfc-editor.org/rfc/rfc8410
const SPKI_PREFIX: [u8; 12] = [
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
];

/// The bytes of an Ed25519 key's `SubjectPublicKeyInfo`.
const SPKI_LEN: usize = SPKI_PREFIX.len() + 32;

/// A server key as TLS carries it with no certificate ([RFC 7250]): its `SubjectPublicKeyInfo`,
/// which a server presents and a client's verifier takes (13-net-stack/udp.md, A server's identity
/// and a client's trust match).
///
/// [RFC 7250]: https://www.rfc-editor.org/rfc/rfc7250
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Spki([u8; SPKI_LEN]);

impl Spki {
    pub(crate) const fn of(key: &ServerKey) -> Self {
        let key = key.as_bytes();
        let mut spki = [0; SPKI_LEN];
        let mut index = 0;
        while index < SPKI_PREFIX.len() {
            spki[index] = SPKI_PREFIX[index];
            index += 1;
        }
        while index < SPKI_LEN {
            spki[index] = key[index - SPKI_PREFIX.len()];
            index += 1;
        }
        Self(spki)
    }

    pub(crate) const fn as_bytes(&self) -> &[u8; SPKI_LEN] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_keys_spki_is_rfc_8410s() {
        // RFC 8410 §10.1's public key,
        // `MCowBQYDK2VwAyEAGb9ECWmEzf6FQbrBZ9w7lshQhqowtrbLDFw4rXAxZuE=`.
        let key = [
            0x19, 0xbf, 0x44, 0x09, 0x69, 0x84, 0xcd, 0xfe, 0x85, 0x41, 0xba, 0xc1, 0x67, 0xdc,
            0x3b, 0x96, 0xc8, 0x50, 0x86, 0xaa, 0x30, 0xb6, 0xb6, 0xcb, 0x0c, 0x5c, 0x38, 0xad,
            0x70, 0x31, 0x66, 0xe1,
        ];
        let der = [
            0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00, 0x19, 0xbf,
            0x44, 0x09, 0x69, 0x84, 0xcd, 0xfe, 0x85, 0x41, 0xba, 0xc1, 0x67, 0xdc, 0x3b, 0x96,
            0xc8, 0x50, 0x86, 0xaa, 0x30, 0xb6, 0xb6, 0xcb, 0x0c, 0x5c, 0x38, 0xad, 0x70, 0x31,
            0x66, 0xe1,
        ];
        assert_eq!(Spki::of(&ServerKey::new(key)).as_bytes(), &der);
    }
}
