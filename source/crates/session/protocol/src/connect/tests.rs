use std::num::NonZeroU32;

use campfire_common::{Fingerprint, MapName, Ticks};
use secp256k1::XOnlyPublicKey;

use super::*;
use crate::delegation::DelegationTerms;
use crate::delegation::error::ScopeError;
use crate::delegation::seed_contribution::SeedContribution;
use crate::engine_release::EngineRelease;
use crate::harness::test_key::TestKey;
use crate::input_rules::InputRules;
use crate::seed_chain::SeedChain;
use crate::session_id::SessionId;
use crate::slot_plan::SlotPlan;

const EXPIRATION: u64 = TestKey::NOW + 60;

fn terms() -> SessionTerms {
    SessionTerms {
        server_key: XOnlyPublicKey::from_byte_array(&[8; 32]).unwrap(),
        tick_hz: NonZeroU32::new(30).unwrap(),
        inputs: InputRules {
            max_delay: Ticks::new(10),
            max_lead: Ticks::new(30),
            max_payload_len: 64,
            max_per_tick: 4,
        },
        seed_commitment: SeedChain::new([6; 32], NonZeroU32::MIN).commitment(),
        release: EngineRelease::new(0, 1, 0),
        mode: Fingerprint::new([5; 32]),
        map: MapName::new("lane").unwrap(),
        dependencies: vec![Fingerprint::new([4; 32])],
        slots: vec![SlotPlan::Player],
        visibility: Vec::new(),
    }
}

/// A delegation from main key 1 to session key 2 in `terms`' session, with `change` applied to
/// its terms first.
fn delegation(change: impl FnOnce(&mut DelegationTerms)) -> Delegation {
    let terms = terms();
    let mut granted = DelegationTerms {
        session_key: TestKey::of(2).x_only_public_key().0,
        server_key: terms.server_key,
        session_id: terms.session_id(),
        seed_contribution: SeedContribution::new([3; 32]),
        expiration: EXPIRATION,
    };
    change(&mut granted);
    Delegation::sign(
        &Secp256k1::new(),
        &TestKey::of(1),
        &granted,
        TestKey::NOW,
        &[0; 32],
    )
}

#[test]
fn a_server_takes_only_an_answer_to_its_challenge_over_its_binding() {
    let secp = Secp256k1::new();
    let challenge = ConnectChallenge::new([9; 32]);
    let binding = ChannelBinding::new([7; 32]);
    let answer = challenge.answer(&secp, &TestKey::of(2), &binding, &[0; 32]);
    let good = delegation(|_| {});
    let check =
        |challenge: ConnectChallenge,
         binding: ChannelBinding,
         delegation: &Delegation,
         answer: &Signature,
         now: u64| { challenge.check(&secp, &terms(), &binding, delegation, answer, now) };
    assert_eq!(
        check(challenge, binding, &good, &answer, TestKey::NOW),
        Ok(())
    );
    // One second before the expiration still holds; at it, the delegation has expired.
    assert_eq!(
        check(challenge, binding, &good, &answer, EXPIRATION - 1),
        Ok(())
    );
    assert_eq!(
        check(challenge, binding, &good, &answer, EXPIRATION),
        Err(ConnectError::Expired)
    );

    let elsewhere = XOnlyPublicKey::from_byte_array(&[9; 32]).unwrap();
    let other_server = delegation(|granted| granted.server_key = elsewhere);
    let other_session = delegation(|granted| granted.session_id = SessionId::new([1; 32]));
    let other_key =
        delegation(|granted| granted.session_key = TestKey::of(3).x_only_public_key().0);
    for (delegation, error) in [
        (&other_server, ConnectError::Scope(ScopeError::OtherServer)),
        (
            &other_session,
            ConnectError::Scope(ScopeError::OtherSession),
        ),
        (&other_key, ConnectError::BadAnswer),
    ] {
        assert_eq!(
            check(challenge, binding, delegation, &answer, TestKey::NOW),
            Err(error)
        );
    }
    // An answer given to another challenge, or over another TLS session's binding, as a relay
    // between two sessions gives, is refused.
    let relayed =
        ConnectChallenge::new([10; 32]).answer(&secp, &TestKey::of(2), &binding, &[0; 32]);
    let other_session = ChannelBinding::new([8; 32]);
    assert_eq!(
        check(challenge, binding, &good, &relayed, TestKey::NOW),
        Err(ConnectError::BadAnswer)
    );
    assert_eq!(
        check(challenge, other_session, &good, &answer, TestKey::NOW),
        Err(ConnectError::BadAnswer)
    );

    // The signed message is the domain, the challenge and the binding, end to end.
    let message = challenge.answer_message(&binding);
    assert_eq!(&message[..19], b"campfire/connect/v2");
    assert_eq!(
        (&message[19..51], &message[51..]),
        (&[9; 32][..], &[7; 32][..])
    );
}
