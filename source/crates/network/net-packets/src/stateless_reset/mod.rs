//! A stateless reset: a datagram that looks like a packet session packet, whose last 16 bytes are
//! the token the peer's endpoint gave in its handshake, so an endpoint that holds no packet session
//! tells its peer at the peer's next packet (RFC 9000 §10.3).

use campfire_common::Secret;

/// The bytes of a reset's token.
pub const TOKEN_LEN: usize = 16;

/// The least length of a reset: a flags byte, 4 bytes that look random, and the token. An endpoint
/// answers no datagram this short with one, so two endpoints never reset each other in a loop.
pub const MIN_LEN: usize = 1 + 4 + TOKEN_LEN;

/// Whether `datagram` is a reset of `token`: long enough, and its last 16 bytes the token, compared
/// as a `Secret` compares, in a time that does not tell how many bytes matched.
pub(crate) fn matches(datagram: &[u8], token: &Secret<TOKEN_LEN>) -> bool {
    let Some(tail) = datagram
        .len()
        .checked_sub(TOKEN_LEN)
        .filter(|_| datagram.len() >= MIN_LEN)
    else {
        return false;
    };
    let tail: [u8; TOKEN_LEN] = datagram[tail..].try_into().expect("the token's last bytes");
    Secret::new(tail) == *token
}

#[cfg(test)]
mod tests;
