use std::sync::Arc;

use rustls::client::Resumption;
use rustls::client::danger::ServerCertVerifier;
use rustls::pki_types::ServerName;
use rustls::version::TLS13;
use rustls::{ConfigBuilder, RootCertStore, WantsVerifier};

use crate::invalid_server_name::InvalidServerName;
use crate::pinned_key_verifier::PinnedKeyVerifier;
use crate::tls_provider;
use campfire_common::TlsKey;

/// The name a pinned client sends, which its verifier never reads.
const PINNED_NAME: &str = "campfire";

/// What a client takes as its server's identity (13-net-stack/udp.md, A server's identity and a
/// client's trust match).
#[derive(Debug, Clone)]
pub struct ServerTrust(Trust);

#[derive(Debug, Clone)]
enum Trust {
    Pinned(TlsKey),
    WebPki(String),
}

impl ServerTrust {
    /// The raw key `key` and no other, as a client gets it from a listing or a lobby.
    pub const fn pinned(key: TlsKey) -> Self {
        Self(Trust::Pinned(key))
    }

    /// A certificate chain for `name` from a root of `webpki-roots`, for a public server with a
    /// domain.
    pub fn web_pki(name: &str) -> Result<Self, InvalidServerName> {
        ServerName::try_from(name).map_err(|_| InvalidServerName {
            name: name.to_owned(),
        })?;
        Ok(Self(Trust::WebPki(name.to_owned())))
    }

    /// The raw key the client pinned; `None` for a client that checks a certificate.
    pub const fn pinned_key(&self) -> Option<TlsKey> {
        match &self.0 {
            Trust::Pinned(key) => Some(*key),
            Trust::WebPki(_) => None,
        }
    }

    /// The name the client sends, and checks a certificate for.
    pub(crate) fn name(&self) -> ServerName<'static> {
        let name = match &self.0 {
            Trust::Pinned(_) => PINNED_NAME,
            Trust::WebPki(name) => name,
        };
        ServerName::try_from(name.to_owned()).expect("a trust's name was checked at its making")
    }

    /// TLS for the client: TLS 1.3, this trust, no key of its own, the ALPN `alpn` alone, and no
    /// resumption, so each handshake checks the server's identity.
    pub(crate) fn tls_config(&self, alpn: &[u8]) -> Arc<rustls::ClientConfig> {
        match &self.0 {
            Trust::Pinned(key) => {
                Self::tls_config_with(Arc::new(PinnedKeyVerifier::new(key)), alpn)
            }
            Trust::WebPki(_) => {
                let config = Self::builder()
                    .with_root_certificates(RootCertStore::from_iter(
                        webpki_roots::TLS_SERVER_ROOTS.iter().cloned(),
                    ))
                    .with_no_client_auth();
                Self::finish(config, alpn)
            }
        }
    }

    /// TLS for a client whose `verifier` checks the server, with the settings of
    /// [`tls_config`](Self::tls_config).
    pub(crate) fn tls_config_with(
        verifier: Arc<dyn ServerCertVerifier>,
        alpn: &[u8],
    ) -> Arc<rustls::ClientConfig> {
        let config = Self::builder()
            .dangerous()
            .with_custom_certificate_verifier(verifier)
            .with_no_client_auth();
        Self::finish(config, alpn)
    }

    fn builder() -> ConfigBuilder<rustls::ClientConfig, WantsVerifier> {
        rustls::ClientConfig::builder_with_provider(tls_provider::get())
            .with_protocol_versions(&[&TLS13])
            .expect("ring offers TLS 1.3")
    }

    fn finish(mut config: rustls::ClientConfig, alpn: &[u8]) -> Arc<rustls::ClientConfig> {
        config.alpn_protocols = vec![alpn.to_vec()];
        config.resumption = Resumption::disabled();
        Arc::new(config)
    }
}
