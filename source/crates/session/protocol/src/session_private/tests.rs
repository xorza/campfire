use std::num::NonZeroU32;

use campfire_codec::BinaryError;

use super::*;

#[test]
fn a_private_record_round_trips_and_other_bytes_are_refused() {
    let private = SessionPrivate {
        seed_chain: SeedChain::new([9; 32], NonZeroU32::new(1024).unwrap()),
    };
    // A buffer that held other bytes is cleared first, so the file decodes.
    let mut bytes = vec![9; 3];
    private.encode(&mut bytes);
    assert_eq!(SessionPrivate::decode(&bytes), Ok(private));
    assert_eq!(
        SessionPrivate::decode(&bytes[1..]),
        Err(TaggedError::OtherTag)
    );
    assert!(matches!(
        SessionPrivate::decode(&bytes[..bytes.len() - 1]),
        Err(TaggedError::Value(_))
    ));
    assert_eq!(
        SessionPrivate::decode(&[bytes.as_slice(), &[0]].concat()),
        Err(TaggedError::Value(BinaryError::NotCanonical))
    );
}
