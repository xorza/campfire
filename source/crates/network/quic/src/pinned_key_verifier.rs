use std::sync::Arc;

use campfire_common::TlsKey;
use quinn_proto::crypto::rustls::QuicClientConfig;
use rustls::client::Resumption;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{WebPkiSupportedAlgorithms, ring, verify_tls13_signature_with_raw_key};
use rustls::pki_types::{CertificateDer, ServerName, SubjectPublicKeyInfoDer, UnixTime};
use rustls::version::TLS13;
use rustls::{CertificateError, DigitallySignedStruct, Error, SignatureScheme};

use crate::server_key::ALPN;
use crate::spki::Spki;

/// A client's check of the server: the raw key the client pinned and no other, with no certificate,
/// chain, name or date to read (11-quic-transport.md, A raw key derived from the server's key). Any
/// other key is refused with `ApplicationVerificationFailure`, which TLS sends as the
/// `access_denied` alert.
#[derive(Debug)]
pub(crate) struct PinnedKeyVerifier {
    spki: Spki,
    algorithms: WebPkiSupportedAlgorithms,
}

impl PinnedKeyVerifier {
    pub(crate) fn new(pin: &TlsKey) -> Self {
        Self {
            spki: Spki::of(pin),
            algorithms: ring::default_provider().signature_verification_algorithms,
        }
    }

    /// TLS for a client that takes this verifier's key alone: TLS 1.3, no key of its own, and no
    /// resumption, so each handshake presents the key and the verifier checks it (D7).
    pub(crate) fn client_config(self) -> Arc<QuicClientConfig> {
        Self::config_of(Arc::new(self))
    }

    fn config_of(verifier: Arc<dyn ServerCertVerifier>) -> Arc<QuicClientConfig> {
        let mut config =
            rustls::ClientConfig::builder_with_provider(Arc::new(ring::default_provider()))
                .with_protocol_versions(&[&TLS13])
                .expect("ring offers TLS 1.3")
                .dangerous()
                .with_custom_certificate_verifier(verifier)
                .with_no_client_auth();
        config.alpn_protocols = vec![ALPN.to_vec()];
        config.resumption = Resumption::disabled();
        Arc::new(
            QuicClientConfig::try_from(config).expect("ring offers QUIC's initial cipher suite"),
        )
    }
}

impl ServerCertVerifier for PinnedKeyVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        if intermediates.is_empty() && end_entity.as_ref() == self.spki.as_bytes() {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(Error::InvalidCertificate(
                CertificateError::ApplicationVerificationFailure,
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        unreachable!("the client offers TLS 1.3 alone")
    }

    /// Checks the signature by the pinned key, which `verify_server_cert` found the server's.
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        _cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls13_signature_with_raw_key(
            message,
            &SubjectPublicKeyInfoDer::from(self.spki.as_bytes().as_slice()),
            dss,
            &self.algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ED25519]
    }

    fn requires_raw_public_keys(&self) -> bool {
        true
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    /// A pinned key's verifier that counts each server key it reads.
    #[derive(Debug)]
    struct CountingVerifier {
        inner: PinnedKeyVerifier,
        checks: Arc<AtomicUsize>,
    }

    impl PinnedKeyVerifier {
        /// TLS for a client whose verifier counts each server key it reads into `checks`.
        pub(crate) fn counting(self, checks: &Arc<AtomicUsize>) -> Arc<QuicClientConfig> {
            Self::config_of(Arc::new(CountingVerifier {
                inner: self,
                checks: Arc::clone(checks),
            }))
        }
    }

    impl ServerCertVerifier for CountingVerifier {
        fn verify_server_cert(
            &self,
            end_entity: &CertificateDer<'_>,
            intermediates: &[CertificateDer<'_>],
            server_name: &ServerName<'_>,
            ocsp_response: &[u8],
            now: UnixTime,
        ) -> Result<ServerCertVerified, Error> {
            self.checks.fetch_add(1, Ordering::Relaxed);
            self.inner.verify_server_cert(
                end_entity,
                intermediates,
                server_name,
                ocsp_response,
                now,
            )
        }

        fn verify_tls12_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            self.inner.verify_tls12_signature(message, cert, dss)
        }

        fn verify_tls13_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            self.inner.verify_tls13_signature(message, cert, dss)
        }

        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            self.inner.supported_verify_schemes()
        }

        fn requires_raw_public_keys(&self) -> bool {
            self.inner.requires_raw_public_keys()
        }
    }
}
