//! The transport parameters each end's TLS carries, encrypted in the server's handshake: the
//! version, and from the server the session's connection ID, its reset token and the client's
//! address tokens for its next connections.

use campfire_net_packets::{ConnectionId, RESET_TOKEN_LEN};

use crate::address_token::TOKEN_LEN;
use crate::handshake_packet::VERSION;

/// The tokens a handshake gives its client: one for its next connection, and one more should that
/// one fail before its own handshake gives new ones.
pub(crate) const TOKENS: usize = 2;

/// What a server tells its client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ServerParams {
    /// The ID that starts each of the client's session datagrams.
    pub(crate) id: ConnectionId,
    pub(crate) reset_token: [u8; RESET_TOKEN_LEN],
    pub(crate) tokens: [[u8; TOKEN_LEN]; TOKENS],
}

impl ServerParams {
    const LEN: usize = 1 + ConnectionId::LEN + RESET_TOKEN_LEN + TOKENS * TOKEN_LEN;

    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(Self::LEN);
        bytes.push(VERSION);
        bytes.extend_from_slice(&self.id.to_bytes());
        bytes.extend_from_slice(&self.reset_token);
        for token in &self.tokens {
            bytes.extend_from_slice(token);
        }
        bytes
    }

    /// The parameters `bytes` holds, of this version and an ID with its first bit clear; `None`
    /// for any other.
    pub(crate) fn decode(bytes: &[u8]) -> Option<Self> {
        const RESET_START: usize = 1 + ConnectionId::LEN;
        const TOKENS_START: usize = RESET_START + RESET_TOKEN_LEN;
        let bytes: &[u8; Self::LEN] = bytes.try_into().ok()?;
        if bytes[0] != VERSION {
            return None;
        }
        let id = ConnectionId::from_bytes(bytes[1..RESET_START].try_into().ok()?)?;
        let reset_token = bytes[RESET_START..TOKENS_START].try_into().ok()?;
        let mut tokens = [[0; TOKEN_LEN]; TOKENS];
        for (index, token) in tokens.iter_mut().enumerate() {
            let start = TOKENS_START + index * TOKEN_LEN;
            token.copy_from_slice(&bytes[start..start + TOKEN_LEN]);
        }
        Some(Self {
            id,
            reset_token,
            tokens,
        })
    }
}

/// What a client tells its server: its version.
pub(crate) fn client() -> Vec<u8> {
    vec![VERSION]
}

/// Whether `bytes` are a client's parameters of this version.
pub(crate) fn is_client(bytes: &[u8]) -> bool {
    bytes == [VERSION]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The server's parameters read back what they wrote, in 1 + 8 + 16 + 2 · 33 = 91 bytes; a
    /// byte short, another version, or an ID with its top bit set is refused.
    #[test]
    fn the_server_parameters_read_what_they_wrote() {
        let params = ServerParams {
            id: ConnectionId::from_random([0x12, 0x34, 0, 0, 0, 0, 0, 0x56]),
            reset_token: [5; RESET_TOKEN_LEN],
            tokens: [[6; TOKEN_LEN], [7; TOKEN_LEN]],
        };
        let bytes = params.encode();
        assert_eq!(bytes.len(), 91);
        assert_eq!(ServerParams::decode(&bytes), Some(params));
        assert_eq!(ServerParams::decode(&bytes[1..]), None);
        let mut other = bytes.clone();
        other[0] = VERSION + 1;
        assert_eq!(ServerParams::decode(&other), None);
        let mut top = bytes;
        top[1] |= 0x80;
        assert_eq!(ServerParams::decode(&top), None);
        assert!(is_client(&client()));
        assert!(!is_client(&[VERSION, 0]));
    }
}
