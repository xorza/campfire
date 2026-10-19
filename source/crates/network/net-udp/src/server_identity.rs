use std::sync::Arc;

use rustls::server::{AlwaysResolvesServerRawPublicKeys, NoServerSessionStorage};
use rustls::sign::SingleCertAndKey;
use rustls::version::TLS13;

use crate::certificate_chain::CertificateChain;
use crate::raw_key_signer::RawKeySigner;
use crate::reset_key::ResetKey;
use crate::server_key::ServerKey;
use crate::server_secret::ServerSecret;
use crate::tls_provider;

/// What a server presents to TLS, and the secret its reset key derives from (13-net-stack/udp.md, A
/// server's identity and a client's trust match).
#[derive(Debug)]
pub struct ServerIdentity {
    secret: ServerSecret,
    presented: Presented,
}

#[derive(Debug)]
enum Presented {
    RawKey(RawKeySigner),
    Certificate(CertificateChain),
}

impl ServerIdentity {
    /// A server that presents the raw key `secret` derives, which its clients pin.
    pub fn raw_key(secret: ServerSecret) -> Self {
        let signer = RawKeySigner::derive(&secret);
        Self {
            secret,
            presented: Presented::RawKey(signer),
        }
    }

    /// A server that presents `chain`, which its clients check for its name; its reset key still
    /// derives from `secret`.
    pub const fn certificate(secret: ServerSecret, chain: CertificateChain) -> Self {
        Self {
            secret,
            presented: Presented::Certificate(chain),
        }
    }

    /// The raw key a client pins; `None` for a server that presents a certificate.
    pub const fn server_key(&self) -> Option<ServerKey> {
        match &self.presented {
            Presented::RawKey(signer) => Some(signer.public()),
            Presented::Certificate(_) => None,
        }
    }

    pub(crate) fn reset_key(&self) -> ResetKey {
        ResetKey::derive(&self.secret)
    }

    /// TLS for the server: TLS 1.3, its identity, no client key, the ALPN `alpn` alone, and no
    /// session ticket, so every client checks the identity.
    pub(crate) fn tls_config(&self, alpn: &[u8]) -> Arc<rustls::ServerConfig> {
        let builder = rustls::ServerConfig::builder_with_provider(tls_provider::get())
            .with_protocol_versions(&[&TLS13])
            .expect("ring offers TLS 1.3")
            .with_no_client_auth();
        let mut config = match &self.presented {
            Presented::RawKey(signer) => builder.with_cert_resolver(Arc::new(
                AlwaysResolvesServerRawPublicKeys::new(Arc::new(signer.certified())),
            )),
            Presented::Certificate(chain) => {
                builder.with_cert_resolver(Arc::new(SingleCertAndKey::from(chain.certified())))
            }
        };
        config.alpn_protocols = vec![alpn.to_vec()];
        config.send_tls13_tickets = 0;
        config.session_storage = Arc::new(NoServerSessionStorage {});
        Arc::new(config)
    }
}
