use std::sync::Arc;

use quinn_proto::crypto::HmacKey;
use ring::hmac;
use ring::rand::{SecureRandom, SystemRandom};
use zeroize::Zeroizing;

use crate::derived_secret::DerivedSecret;

/// The key of an endpoint's stateless resets: a reset's token is its HMAC of a connection id. A
/// server's is derived from its key, so a server started again resets the connections its last
/// run held at their next packet (design 15, D8); a client's is drawn at random.
#[derive(Debug)]
pub struct ResetKey(hmac::Key);

impl ResetKey {
    /// The reset key of the server whose own key's secret is `secret`.
    pub fn derive(secret: &Zeroizing<[u8; 32]>) -> ResetKey {
        ResetKey::new(&DerivedSecret::ResetKey.of(secret))
    }

    pub(crate) fn new(secret: &Zeroizing<[u8; 32]>) -> ResetKey {
        ResetKey(hmac::Key::new(hmac::HMAC_SHA256, secret.as_slice()))
    }

    pub(crate) fn random() -> ResetKey {
        let mut secret = Zeroizing::new([0; 32]);
        SystemRandom::new()
            .fill(secret.as_mut_slice())
            .expect("the OS gives random bytes");
        ResetKey::new(&secret)
    }

    pub(crate) fn into_hmac(self) -> Arc<dyn HmacKey> {
        Arc::new(self.0)
    }
}
