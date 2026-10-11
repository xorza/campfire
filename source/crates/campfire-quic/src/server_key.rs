use std::sync::Arc;

use campfire_common::TlsKey;

use quinn_proto::crypto::rustls::QuicServerConfig;
use ring::signature::{Ed25519KeyPair, KeyPair};
use rustls::crypto::ring as provider;
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use rustls::server::{AlwaysResolvesServerRawPublicKeys, NoServerSessionStorage};
use rustls::sign::{CertifiedKey, SigningKey};
use rustls::version::TLS13;
use zeroize::Zeroizing;

use crate::derived_secret::DerivedSecret;
use crate::spki::Spki;

/// The ALPN of every Campfire connection, which RFC 9001 requires (design 15, D7).
pub(crate) const ALPN: &[u8] = b"campfire";

/// A one-key PKCS#8 v1 `PrivateKeyInfo` of Ed25519 before its 32-byte seed ([RFC 8410] §7).
///
/// [RFC 8410]: https://www.rfc-editor.org/rfc/rfc8410
const PKCS8_PREFIX: [u8; 16] = [
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
];
const PKCS8_LEN: usize = PKCS8_PREFIX.len() + 32;

/// The server's TLS identity: an Ed25519 key with no certificate, whose seed derives from the
/// server's own key (design 15, D6).
#[derive(Debug)]
pub struct ServerKey {
    signing: Arc<dyn SigningKey>,
    public: TlsKey,
}

impl ServerKey {
    /// The TLS key of the server whose own key's secret is `secret`.
    pub fn derive(secret: &Zeroizing<[u8; 32]>) -> ServerKey {
        ServerKey::from_seed(&DerivedSecret::TlsSeed.of(secret))
    }

    pub(crate) fn from_seed(seed: &Zeroizing<[u8; 32]>) -> ServerKey {
        let pair = Ed25519KeyPair::from_seed_unchecked(seed.as_slice())
            .expect("any 32 bytes are an Ed25519 seed");
        let public = TlsKey::new(
            pair.public_key()
                .as_ref()
                .try_into()
                .expect("an Ed25519 public key is 32 bytes"),
        );
        let pkcs8 = ServerKey::pkcs8(seed);
        let signing = provider::sign::any_eddsa_type(&PrivatePkcs8KeyDer::from(&pkcs8[..]))
            .expect("ring loads its own Ed25519 encoding");
        ServerKey { signing, public }
    }

    /// The public key, which a listing names and a client pins.
    pub const fn public(&self) -> TlsKey {
        self.public
    }

    /// TLS for the server: TLS 1.3, the raw key alone, no client key, and no session ticket, so
    /// every client's verifier reads the key (D7).
    pub(crate) fn server_config(&self) -> Arc<QuicServerConfig> {
        let certified = CertifiedKey::new(
            vec![CertificateDer::from(
                Spki::of(&self.public).as_bytes().to_vec(),
            )],
            Arc::clone(&self.signing),
        );
        let mut config =
            rustls::ServerConfig::builder_with_provider(Arc::new(provider::default_provider()))
                .with_protocol_versions(&[&TLS13])
                .expect("ring offers TLS 1.3")
                .with_no_client_auth()
                .with_cert_resolver(Arc::new(AlwaysResolvesServerRawPublicKeys::new(Arc::new(
                    certified,
                ))));
        config.alpn_protocols = vec![ALPN.to_vec()];
        config.send_tls13_tickets = 0;
        config.session_storage = Arc::new(NoServerSessionStorage {});
        Arc::new(
            QuicServerConfig::try_from(config).expect("ring offers QUIC's initial cipher suite"),
        )
    }

    fn pkcs8(seed: &Zeroizing<[u8; 32]>) -> Zeroizing<[u8; PKCS8_LEN]> {
        let mut pkcs8 = Zeroizing::new([0; PKCS8_LEN]);
        pkcs8[..PKCS8_PREFIX.len()].copy_from_slice(&PKCS8_PREFIX);
        pkcs8[PKCS8_PREFIX.len()..].copy_from_slice(seed.as_slice());
        pkcs8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seeds_encodings_are_rfc_8410s() {
        // RFC 8410 §10.3's private key, `MC4CAQAwBQYDK2VwBCIEINTuctv5E1hK1bbY8fdp+K06/nwoy/HU++CXqI9EdVhC`,
        // whose public key is §10.1's.
        let seed = Zeroizing::new([
            0xd4, 0xee, 0x72, 0xdb, 0xf9, 0x13, 0x58, 0x4a, 0xd5, 0xb6, 0xd8, 0xf1, 0xf7, 0x69,
            0xf8, 0xad, 0x3a, 0xfe, 0x7c, 0x28, 0xcb, 0xf1, 0xd4, 0xfb, 0xe0, 0x97, 0xa8, 0x8f,
            0x44, 0x75, 0x58, 0x42,
        ]);
        let der = [
            0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22,
            0x04, 0x20, 0xd4, 0xee, 0x72, 0xdb, 0xf9, 0x13, 0x58, 0x4a, 0xd5, 0xb6, 0xd8, 0xf1,
            0xf7, 0x69, 0xf8, 0xad, 0x3a, 0xfe, 0x7c, 0x28, 0xcb, 0xf1, 0xd4, 0xfb, 0xe0, 0x97,
            0xa8, 0x8f, 0x44, 0x75, 0x58, 0x42,
        ];
        assert_eq!(*ServerKey::pkcs8(&seed), der);
        let public = [
            0x19, 0xbf, 0x44, 0x09, 0x69, 0x84, 0xcd, 0xfe, 0x85, 0x41, 0xba, 0xc1, 0x67, 0xdc,
            0x3b, 0x96, 0xc8, 0x50, 0x86, 0xaa, 0x30, 0xb6, 0xb6, 0xcb, 0x0c, 0x5c, 0x38, 0xad,
            0x70, 0x31, 0x66, 0xe1,
        ];
        assert_eq!(ServerKey::from_seed(&seed).public(), TlsKey::new(public));
    }

    #[test]
    fn a_server_key_gives_one_tls_key_and_two_give_two() {
        let derive = |byte| ServerKey::derive(&Zeroizing::new([byte; 32])).public();
        assert_eq!(derive(1), derive(1));
        assert_ne!(derive(1), derive(2));
    }
}
