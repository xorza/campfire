use campfire_net_packets::{ConnectionId, RESET_TOKEN_LEN};
use ring::hmac;

use crate::derived_secret::DerivedSecret;
use crate::server_secret::ServerSecret;

/// The key of a server's stateless resets: the token of a packet session is its HMAC of the packet
/// session's connection ID. It derives from the server's secret, so a server started again resets
/// the packet sessions its last run held at their next packet (11-net-stack/udp.md, A stateless
/// reset ends the packet sessions of a server that started again).
#[derive(Debug)]
pub(crate) struct ResetKey(hmac::Key);

impl ResetKey {
    /// The reset key of the server whose secret is `secret`.
    pub(crate) fn derive(secret: &ServerSecret) -> Self {
        let key = DerivedSecret::ResetKey.of(secret);
        Self(hmac::Key::new(hmac::HMAC_SHA256, key.as_bytes()))
    }

    /// The reset token of the packet session whose connection ID is `id`: the first 16 bytes of its
    /// HMAC.
    pub(crate) fn token(&self, id: ConnectionId) -> [u8; RESET_TOKEN_LEN] {
        let tag = hmac::sign(&self.0, &id.to_bytes());
        let mut token = [0; RESET_TOKEN_LEN];
        token.copy_from_slice(&tag.as_ref()[..RESET_TOKEN_LEN]);
        token
    }
}

#[cfg(test)]
mod tests;
