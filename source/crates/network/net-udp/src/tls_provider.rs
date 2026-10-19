//! The crypto provider of every TLS config the crate makes.

use std::sync::Arc;

use rustls::crypto::{CryptoProvider, ring};

/// `ring`'s provider with ChaCha20-Poly1305 alone, whose packets need no key update in a session
/// (13-net-stack/udp.md, The AEAD is ChaCha20-Poly1305 only), and whose random numbers are the
/// OS's.
pub(crate) fn get() -> Arc<CryptoProvider> {
    Arc::new(CryptoProvider {
        cipher_suites: vec![ring::cipher_suite::TLS13_CHACHA20_POLY1305_SHA256],
        ..ring::default_provider()
    })
}
