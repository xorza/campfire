//! A stateless reset: a datagram that looks like a session packet, whose last 16 bytes are the
//! token the peer's endpoint gave in its handshake, so an endpoint that holds no session tells its
//! peer at the peer's next packet (RFC 9000 §10.3).

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
mod tests {
    use super::*;

    /// A datagram of 21 bytes ending in the token is a reset; one of 20 is not, nor one whose last
    /// byte differs.
    #[test]
    fn a_reset_ends_in_its_token_and_is_long_enough() {
        let token = [9; TOKEN_LEN];
        let mut datagram = vec![0; 5];
        datagram.extend_from_slice(&token);
        assert!(matches(&datagram, &token));
        assert!(!matches(&datagram[1..], &token));
        *datagram.last_mut().unwrap() = 8;
        assert!(!matches(&datagram, &token));
    }
}
