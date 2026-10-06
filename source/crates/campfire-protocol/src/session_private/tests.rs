use std::num::NonZeroU32;

use campfire_common::{Fingerprint, Ticks};
use secp256k1::{Keypair, Secp256k1, SecretKey};

use super::*;
use crate::slot_plan::SlotPlan;

#[test]
fn a_private_record_round_trips_and_other_bytes_are_refused() {
    let secret = SecretKey::from_byte_array(&[8; 32]).unwrap();
    let key = Keypair::from_secret_key(&Secp256k1::new(), &secret);
    let seed_chain = SeedChain::new([9; 32], NonZeroU32::new(1024).unwrap());
    let private = SessionPrivate {
        seed_chain,
        terms: SessionTerms {
            server_key: key.x_only_public_key().0,
            tick_hz: NonZeroU32::new(30).unwrap(),
            max_input_delay: Ticks::new(3),
            max_input_lead: Ticks::new(3),
            max_payload_len: 64,
            max_inputs_per_tick: 4,
            seed_commitment: seed_chain.commitment(),
            release: "0.1.0".to_owned(),
            mode: Fingerprint::new([1; 32]),
            dependencies: vec![Fingerprint::new([2; 32])],
            slots: vec![SlotPlan::Player, SlotPlan::Open],
        },
    };
    let bytes = private.encode();
    assert_eq!(SessionPrivate::decode(&bytes), Ok(private));
    assert_eq!(
        SessionPrivate::decode(&bytes[1..]),
        Err(SessionPrivateError::NotPrivate)
    );
    assert!(matches!(
        SessionPrivate::decode(&bytes[..bytes.len() - 1]),
        Err(SessionPrivateError::Malformed(_))
    ));
    assert_eq!(
        SessionPrivate::decode(&[bytes.as_slice(), &[0]].concat()),
        Err(SessionPrivateError::Trailing)
    );
}
