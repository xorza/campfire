use super::*;

#[test]
fn a_key_skips_bytes_that_are_no_secret_key() {
    // Zero is no secret key: the next draw, 32 bytes of 7, is the key.
    let mut draws = [[0; 32], [7; 32]].into_iter();
    let key = RandomKey::generate(|bytes| *bytes = draws.next().unwrap());
    let seven = SecretKey::from_byte_array(&[7; 32]).unwrap();
    assert_eq!(key, Keypair::from_secret_key(&Secp256k1::new(), &seven));
    assert_eq!(draws.next(), None);
}
