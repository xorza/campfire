use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{WebPkiSupportedAlgorithms, ring, verify_tls13_signature_with_raw_key};
use rustls::pki_types::{CertificateDer, ServerName, SubjectPublicKeyInfoDer, UnixTime};
use rustls::{CertificateError, DigitallySignedStruct, Error, SignatureScheme};

use crate::spki::Spki;
use campfire_common::TlsKey;

/// A client's check of the server: the raw key the client pinned and no other, with no certificate,
/// chain, name or date to read (11-net-stack/udp.md, A server's identity and a client's trust
/// match). Any other key is refused with `ApplicationVerificationFailure`, which TLS sends as the
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
