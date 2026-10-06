use std::num::NonZeroU32;

use campfire_common::{Fingerprint, NotHex, Ticks};
use secp256k1::{SecretKey, XOnlyPublicKey};

use crate::delegation::DelegationTerms;
use crate::delegation::error::ScopeError;
use crate::seed_chain::SeedChain;
use crate::session_id::SessionId;

use super::*;
use crate::slot_plan::SlotPlan;

const NOW: u64 = 1_700_000_000;
const EXPIRATION: u64 = NOW + 60;

fn keypair(byte: u8) -> Keypair {
    let secret = SecretKey::from_byte_array(&[byte; 32]).unwrap();
    Keypair::from_secret_key(&Secp256k1::new(), &secret)
}

fn terms() -> SessionTerms {
    SessionTerms {
        server_key: XOnlyPublicKey::from_byte_array(&[8; 32]).unwrap(),
        tick_hz: NonZeroU32::new(30).unwrap(),
        max_input_delay: Ticks::new(10),
        max_input_lead: Ticks::new(30),
        max_payload_len: 64,
        max_inputs_per_tick: 4,
        seed_commitment: SeedChain::new([6; 32], NonZeroU32::MIN).commitment(),
        release: "0.1.0".to_owned(),
        mode: Fingerprint::new([5; 32]),
        dependencies: vec![Fingerprint::new([4; 32])],
        slots: vec![SlotPlan::Player],
    }
}

/// A delegation from main key 1 to session key 2 in `terms`' session, with `change` applied to
/// its terms first.
fn delegation(change: impl FnOnce(&mut DelegationTerms)) -> Delegation {
    let terms = terms();
    let mut granted = DelegationTerms {
        session_key: keypair(2).x_only_public_key().0,
        server_key: terms.server_key,
        session_id: terms.session_id(),
        seed_contribution: [3; 32],
        expiration: EXPIRATION,
    };
    change(&mut granted);
    Delegation::sign(&Secp256k1::new(), &keypair(1), &granted, NOW, &[0; 32])
}

#[test]
fn a_server_takes_only_an_answer_to_its_challenge_over_its_certificate() {
    let secp = Secp256k1::new();
    let challenge = ConnectChallenge::new([9; 32]);
    let certificate = CertificateHash::new([7; 32]);
    let answer = challenge.answer(&secp, &keypair(2), &certificate, &[0; 32]);
    let good = delegation(|_| {});
    let check = |challenge: ConnectChallenge,
                 certificate: CertificateHash,
                 delegation: &Delegation,
                 answer: &Signature,
                 now: u64| {
        challenge.check(&secp, &terms(), &certificate, delegation, answer, now)
    };
    assert_eq!(check(challenge, certificate, &good, &answer, NOW), Ok(()));
    // One second before the expiration still holds; at it, the delegation has expired.
    assert_eq!(
        check(challenge, certificate, &good, &answer, EXPIRATION - 1),
        Ok(())
    );
    assert_eq!(
        check(challenge, certificate, &good, &answer, EXPIRATION),
        Err(ConnectError::Expired)
    );

    let elsewhere = XOnlyPublicKey::from_byte_array(&[9; 32]).unwrap();
    let other_server = delegation(|granted| granted.server_key = elsewhere);
    let other_session = delegation(|granted| granted.session_id = SessionId::new([1; 32]));
    let other_key = delegation(|granted| granted.session_key = keypair(3).x_only_public_key().0);
    for (delegation, error) in [
        (&other_server, ConnectError::Scope(ScopeError::OtherServer)),
        (
            &other_session,
            ConnectError::Scope(ScopeError::OtherSession),
        ),
        (&other_key, ConnectError::BadAnswer),
    ] {
        assert_eq!(
            check(challenge, certificate, delegation, &answer, NOW),
            Err(error)
        );
    }
    // An answer given to another challenge, or over another server's certificate, is refused.
    let relayed =
        ConnectChallenge::new([10; 32]).answer(&secp, &keypair(2), &certificate, &[0; 32]);
    let elsewhere = CertificateHash::new([8; 32]);
    assert_eq!(
        check(challenge, certificate, &good, &relayed, NOW),
        Err(ConnectError::BadAnswer)
    );
    assert_eq!(
        check(challenge, elsewhere, &good, &answer, NOW),
        Err(ConnectError::BadAnswer)
    );

    // The hash reads back what it writes, as 64 lowercase hex digits.
    let written = certificate.to_string();
    assert_eq!(written, "07".repeat(32));
    assert_eq!(written.parse(), Ok(certificate));
    assert_eq!("07".parse::<CertificateHash>(), Err(NotHex));

    // The signed message is the domain, the challenge and the hash, end to end.
    let message = challenge.answer_message(&certificate);
    assert_eq!(&message[..19], b"campfire/connect/v1");
    assert_eq!(
        (&message[19..51], &message[51..]),
        (&[9; 32][..], &[7; 32][..])
    );
}
