use std::num::NonZeroU32;

use campfire_common::{Fingerprint, MapName, Ticks};

use super::*;
use crate::engine_release::EngineRelease;
use crate::harness::test_key::TestKey;
use crate::input_rules::InputRules;
use crate::slot_plan::SlotPlan;

#[test]
fn a_private_record_round_trips_and_other_bytes_are_refused() {
    let key = TestKey::of(8);
    let seed_chain = SeedChain::new([9; 32], NonZeroU32::new(1024).unwrap());
    let private = SessionPrivate {
        seed_chain,
        terms: SessionTerms {
            server_key: key.x_only_public_key().0,
            tick_hz: NonZeroU32::new(30).unwrap(),
            inputs: InputRules {
                max_delay: Ticks::new(3),
                max_lead: Ticks::new(3),
                max_payload_len: 64,
                max_per_tick: 4,
            },
            seed_commitment: seed_chain.commitment(),
            release: EngineRelease::new(0, 1, 0),
            mode: Fingerprint::new([1; 32]),
            map: MapName::new("lane").unwrap(),
            dependencies: vec![Fingerprint::new([2; 32])],
            slots: vec![SlotPlan::Player, SlotPlan::Open],
            visibility: Vec::new(),
        },
    };
    // A buffer that held other bytes is cleared first, so the file decodes.
    let mut bytes = vec![9; 3];
    private.encode(&mut bytes);
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
