use campfire_net_packets::{ConnectionId, RESET_TOKEN_LEN};
use ring::hmac;

use crate::derived_secret::DerivedSecret;
use crate::server_secret::ServerSecret;

/// The key of a server's stateless resets: the token of a session is its HMAC of the session's
/// connection ID. It derives from the server's secret, so a server started again resets the
/// sessions its last run held at their next packet (13-net-stack/udp.md, A stateless reset ends the
/// sessions of a server that started again).
#[derive(Debug)]
pub(crate) struct ResetKey(hmac::Key);

impl ResetKey {
    /// The reset key of the server whose secret is `secret`.
    pub(crate) fn derive(secret: &ServerSecret) -> Self {
        let key = DerivedSecret::ResetKey.of(secret);
        Self(hmac::Key::new(hmac::HMAC_SHA256, key.as_slice()))
    }

    /// The reset token of the session whose connection ID is `id`: the first 16 bytes of its HMAC.
    pub(crate) fn token(&self, id: ConnectionId) -> [u8; RESET_TOKEN_LEN] {
        let tag = hmac::sign(&self.0, &id.to_bytes());
        let mut token = [0; RESET_TOKEN_LEN];
        token.copy_from_slice(&tag.as_ref()[..RESET_TOKEN_LEN]);
        token
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeroize::Zeroizing;

    /// A token is the first 16 bytes of HMAC-SHA256 of the ID's 8 bytes, under the key the secret
    /// derives: Python's `hmac.new(reset_key, bytes([0] * 7 + [7]), sha256)` for ID 7 and the
    /// secret 0x00 to 0x1f. The same secret gives the same token, and another ID another.
    #[test]
    fn a_token_is_the_hmac_of_its_id_under_the_derived_key() {
        let secret = ServerSecret::new(Zeroizing::new(core::array::from_fn(|index| {
            u8::try_from(index).unwrap()
        })));
        let key = ResetKey::derive(&secret);
        let id = |last| ConnectionId::from_random([0, 0, 0, 0, 0, 0, 0, last]);
        assert_eq!(key.token(id(7)), RESET_TOKEN_7);
        assert_ne!(key.token(id(8)), key.token(id(7)));
    }

    const RESET_TOKEN_7: [u8; RESET_TOKEN_LEN] = [
        0xb3, 0xcd, 0xef, 0x1d, 0x51, 0xa0, 0x8a, 0x9f, 0xf2, 0xf9, 0xc0, 0x7a, 0xdb, 0x0c, 0x10,
        0x29,
    ];
}
