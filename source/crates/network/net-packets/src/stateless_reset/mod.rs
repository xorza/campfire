//! A stateless reset: a datagram that looks like a packet session packet, whose last 16 bytes are
//! the token the peer's endpoint gave in its handshake, so an endpoint that holds no packet session
//! tells its peer at the peer's next packet (RFC 9000 §10.3).

/// The bytes of a reset's token.
pub const TOKEN_LEN: usize = 16;

/// The least length of a reset: a flags byte, 4 bytes that look random, and the token. An endpoint
/// answers no datagram this short with one, so two endpoints never reset each other in a loop.
pub const MIN_LEN: usize = 1 + 4 + TOKEN_LEN;

/// Whether `datagram` is a reset of `token`: long enough, and its last 16 bytes the token, compared
/// in a time that does not tell how many bytes matched.
pub(crate) fn matches(datagram: &[u8], token: &[u8; TOKEN_LEN]) -> bool {
    if datagram.len() < MIN_LEN {
        return false;
    }
    datagram[datagram.len() - TOKEN_LEN..]
        .iter()
        .zip(token)
        .fold(0, |difference, (byte, token)| difference | (byte ^ token))
        == 0
}

#[cfg(test)]
mod tests;
