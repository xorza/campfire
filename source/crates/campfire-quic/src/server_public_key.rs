/// The `SubjectPublicKeyInfo` of an Ed25519 key before its 32 bytes ([RFC 8410] §4): the
/// algorithm `id-Ed25519` and the bit string's header.
///
/// [RFC 8410]: https://www.rfc-editor.org/rfc/rfc8410
const SPKI_PREFIX: [u8; 12] = [
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
];

/// The bytes of an Ed25519 key's `SubjectPublicKeyInfo`.
pub(crate) const SPKI_LEN: usize = SPKI_PREFIX.len() + 32;

/// The public half of a server's TLS key, which a listing names and a client pins (design 15, D6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerPublicKey([u8; 32]);

impl ServerPublicKey {
    pub const fn new(bytes: [u8; 32]) -> ServerPublicKey {
        ServerPublicKey(bytes)
    }

    pub const fn bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// The key as TLS presents it with no certificate ([RFC 7250]).
    ///
    /// [RFC 7250]: https://www.rfc-editor.org/rfc/rfc7250
    pub(crate) const fn spki(&self) -> [u8; SPKI_LEN] {
        let mut spki = [0; SPKI_LEN];
        let mut index = 0;
        while index < SPKI_PREFIX.len() {
            spki[index] = SPKI_PREFIX[index];
            index += 1;
        }
        while index < SPKI_LEN {
            spki[index] = self.0[index - SPKI_PREFIX.len()];
            index += 1;
        }
        spki
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_keys_spki_is_rfc_8410s() {
        // RFC 8410 §10.1's public key, `MCowBQYDK2VwAyEAGb9ECWmEzf6FQbrBZ9w7lshQhqowtrbLDFw4rXAxZuE=`.
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
        assert_eq!(ServerPublicKey::new(key).spki(), der);
    }
}
