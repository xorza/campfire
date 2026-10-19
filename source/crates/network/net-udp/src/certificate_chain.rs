use std::sync::Arc;

use rustls::crypto::ring;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::sign::CertifiedKey;

use crate::certificate_error::CertificateError;

/// A server's certificate chain and its private key, which a client of
/// [`ServerTrust::web_pki`](crate::server_trust::ServerTrust::web_pki) checks for its name.
#[derive(Debug, Clone)]
pub struct CertificateChain(Arc<CertifiedKey>);

impl CertificateChain {
    /// The chain of `certificates`, each in DER, the server's own first, and its `private_key` in
    /// DER: PKCS#8, SEC1 or PKCS#1.
    pub fn new(
        certificates: impl IntoIterator<Item = Vec<u8>>,
        private_key: Vec<u8>,
    ) -> Result<Self, CertificateError> {
        let certificates: Vec<CertificateDer<'static>> =
            certificates.into_iter().map(CertificateDer::from).collect();
        Self::of(certificates, private_key)
    }

    fn of(
        certificates: Vec<CertificateDer<'static>>,
        private_key: Vec<u8>,
    ) -> Result<Self, CertificateError> {
        if certificates.is_empty() {
            return Err(CertificateError::Empty);
        }
        let key = PrivateKeyDer::try_from(private_key).map_err(|_| CertificateError::KeyFormat)?;
        let provider = ring::default_provider();
        CertifiedKey::from_der(certificates, key, &provider)
            .map(|certified| Self(Arc::new(certified)))
            .map_err(CertificateError::Key)
    }

    pub(crate) fn certified(&self) -> Arc<CertifiedKey> {
        Arc::clone(&self.0)
    }
}
