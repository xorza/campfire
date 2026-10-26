use super::*;

/// A datagram of 21 bytes ending in the token is a reset; one of 20 is not, nor one whose last
/// byte differs.
#[test]
fn a_reset_ends_in_its_token_and_is_long_enough() {
    let token = Secret::new([9; TOKEN_LEN]);
    let mut datagram = vec![0; 5];
    datagram.extend_from_slice(token.as_bytes());
    assert!(matches(&datagram, &token));
    assert!(!matches(&datagram[1..], &token));
    *datagram.last_mut().unwrap() = 8;
    assert!(!matches(&datagram, &token));
}
