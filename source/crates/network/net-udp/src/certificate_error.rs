/// Why a certificate chain cannot be a server's identity.
#[derive(Debug, thiserror::Error)]
pub enum CertificateError {
    /// The chain holds no certificate.
    #[error("a certificate chain holds at least the server's own certificate")]
    Empty,
    /// The private key is in no DER format TLS reads: PKCS#8, SEC1 or PKCS#1.
    #[error("the private key is in no known DER format")]
    KeyFormat,
    /// TLS refused the key, or found that it does not match the first certificate.
    #[error("TLS refuses the private key")]
    Key(#[source] rustls::Error),
}
